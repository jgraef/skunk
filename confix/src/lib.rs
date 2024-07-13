use toml_edit::{DocumentMut, Value};


pub struct ConfigFile {

}

struct Inner {
    document: DocumentMut,
}



pub trait FromToml: Sized {
    fn from_toml(value: &Value) -> Result<Self, Error>;
}

pub enum Error {

}