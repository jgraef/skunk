#[derive(Debug)]
pub struct Ids {
    scope: u8,
    next: u32,
}

impl Ids {
    pub const SCOPE_SERVER: u8 = 1;
    pub const SCOPE_CLIENT: u8 = 2;

    #[inline]
    pub fn new(scope: u8) -> Self {
        Self { scope, next: 1 }
    }

    #[inline]
    pub fn next(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id << 8 | u32::from(self.scope)
    }
}
