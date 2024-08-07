use std::{
    ffi::OsStr,
    io::Write,
    process::Stdio,
};

use tempfile::NamedTempFile;
use tokio::{
    io::{
        AsyncBufReadExt,
        BufReader,
    },
    process::Command,
    sync::{
        oneshot,
        watch,
    },
    task::JoinHandle,
};
use tracing::Instrument;

use super::interface::Interface;

#[derive(Debug, thiserror::Error)]
#[error("hostapd error")]
pub enum Error {
    Io(#[from] std::io::Error),
    HostApdFailed,
}

#[derive(Clone, Copy, Debug, strum::Display, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum Driver {
    HostAp,
    Wired,
    Nl80211,
    Bsd,
}

impl Default for Driver {
    fn default() -> Self {
        Self::Nl80211
    }
}

#[derive(Clone, Copy, Debug, strum::Display, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum HwMode {
    A,
    B,
    G,
}

impl Default for HwMode {
    fn default() -> Self {
        Self::G
    }
}

pub struct Builder<'a> {
    hostapd_bin: Option<&'a OsStr>,
    interface: &'a Interface,
    driver: Driver,
    ssid: &'a str,
    country_code: &'a str,
    hw_mode: HwMode,
    channel: Option<u8>,
    password: Option<&'a str>,
    ready: Option<watch::Sender<bool>>,
}

impl<'a> Builder<'a> {
    pub fn new(interface: &'a Interface, country_code: &'a str) -> Self {
        Self {
            hostapd_bin: None,
            interface,
            driver: Default::default(),
            ssid: "skunk",
            country_code,
            hw_mode: Default::default(),
            channel: None,
            password: None,
            ready: None,
        }
    }

    pub fn with_hostapd(mut self, bin: &'a OsStr) -> Self {
        self.hostapd_bin = Some(bin);
        self
    }

    pub fn with_driver(mut self, driver: Driver) -> Self {
        self.driver = driver;
        self
    }

    pub fn with_ssid(mut self, ssid: &'a str) -> Self {
        self.ssid = ssid;
        self
    }

    pub fn with_hw_mode(mut self, hw_mode: HwMode) -> Self {
        self.hw_mode = hw_mode;
        self
    }

    pub fn with_channel(mut self, channel: u8) -> Self {
        self.channel = Some(channel);
        self
    }

    pub fn with_password(mut self, password: &'a str) -> Self {
        self.password = Some(password);
        self
    }

    pub fn write_config(&self, mut writer: impl Write) -> Result<(), Error> {
        writeln!(writer, "interface={}", self.interface.name())?;
        writeln!(writer, "driver={}", <&'static str>::from(self.driver))?;
        writeln!(writer, "ssid={}", self.ssid)?;
        writeln!(writer, "country_code={}", self.country_code)?;
        writeln!(writer, "hw_mode={}", <&'static str>::from(self.hw_mode))?;
        if let Some(channel) = self.channel {
            writeln!(writer, "channel={channel}")?;
        }
        if let Some(password) = &self.password {
            writeln!(writer, "wpa=2")?;
            writeln!(writer, "wpa_passphrase={}", password)?;
            writeln!(writer, "wpa_key_mgmt=WPA-PSK")?;
            writeln!(writer, "wpa_pairwise=TKIP")?;
            writeln!(writer, "rsn_pairwise=CCMP")?;
        }
        Ok(())
    }

    pub fn start(self) -> Result<HostApd, Error> {
        let mut cfg_file = NamedTempFile::with_prefix("hostapd.")?;
        self.write_config(&mut cfg_file)?;

        let bin = self.hostapd_bin.unwrap_or(OsStr::new("hostapd"));
        let mut process = Command::new(bin)
            .arg(cfg_file.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()) // hostapd only logs to stdout
            .kill_on_drop(true)
            .spawn()?;

        let span = tracing::debug_span!("hostapd", pid = process.id().unwrap());
        tracing::debug!(parent: &span, "spawning hostapd");

        let (ready_tx, ready_rx) = watch::channel(false);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let join_handle = tokio::spawn(
            async move {
                // move temp config file here, so it is only deleted once the process
                // terminates.
                let _cfg_file = cfg_file;

                let reader = BufReader::new(process.stdout.as_mut().expect("no stdout"));
                let mut lines = reader.lines();

                loop {
                    tokio::select! {
                        result = lines.next_line() => {
                            if let Some(line) = result? {
                                if line.contains("AP-ENABLED") {
                                    let _ = ready_tx.send(true);
                                }
                                tracing::debug!("{}", line);
                            }
                            else {
                                // EOF on stdout
                                break;
                            }
                        },
                        _ = &mut shutdown_rx => {
                            // either the user sent a shutdown Signal through [`HostApd::stop`], or the sender was dropped.
                            // either case, we're done.
                            break;
                        },
                    };
                }

                tracing::debug!("killing hostapd");
                process.kill().await?;
                Ok::<(), Error>(())
            }
            .instrument(span),
        );

        Ok(HostApd {
            join_handle,
            shutdown_tx,
            ready_rx,
        })
    }
}

pub struct HostApd {
    join_handle: JoinHandle<Result<(), Error>>,
    shutdown_tx: oneshot::Sender<()>,
    ready_rx: watch::Receiver<bool>,
}

impl HostApd {
    pub async fn ready(&mut self) -> Result<(), Error> {
        self.ready_rx
            .wait_for(|ready| *ready)
            .await
            .map(|_| ())
            .map_err(|_| {
                // hostapd failed before it was ready.
                Error::HostApdFailed
            })
    }

    pub async fn stop(self) -> Result<(), Error> {
        let _ = self.shutdown_tx.send(());
        self.join_handle.await.ok().transpose()?;
        Ok(())
    }
}
