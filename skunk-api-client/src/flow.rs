use skunk_api_protocol::flow::Flow;

#[derive(Clone, Debug)]
pub(crate) enum Event {
    BeginFlow { flow: Flow },
}
