#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubString {
    pub s: String,    // contents expected to be found between start and end
    pub start: usize, // byte index in the original buffer
}

impl SubString {
    pub fn new(buffer: &str, substring: &str) -> anyhow::Result<Self> {
        let substring_ptr = substring.as_ptr() as usize;
        let buf_ptr = buffer.as_ptr() as usize;

        if substring_ptr < buf_ptr || substring_ptr + substring.len() > buf_ptr + buffer.len() {
            return Err(anyhow::anyhow!("Substring not found in buffer"));
        }

        let start = substring_ptr - buf_ptr;

        Ok(Self {
            s: substring.to_string(),
            start,
        })
    }

    pub fn from_parts(s: impl Into<String>, start: usize) -> Self {
        Self { s: s.into(), start }
    }

    pub fn end(&self) -> usize {
        self.start + self.s.len()
    }

    pub fn overlaps_with(&self, other: &SubString) -> bool {
        (self.start..=self.end()).contains(&other.start)
            || (other.start..=other.end()).contains(&self.start)
    }
}

impl AsRef<str> for SubString {
    fn as_ref(&self) -> &str {
        &self.s
    }
}

impl PartialEq<&str> for SubString {
    fn eq(&self, other: &&str) -> bool {
        self.s == *other
    }
}
