use anyhow::{Context as _, Result, anyhow, bail};

pub struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    pub const fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    pub fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let (head, tail) = self
            .rest
            .split_at_checked(len)
            .with_context(|| truncated(len, self.rest.len()))?;
        self.rest = tail;
        Ok(head)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_ignored| anyhow!("cannot read {N} bytes"))
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(u8::from_le_bytes(self.array::<1>()?))
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array::<2>()?))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array::<4>()?))
    }

    pub fn sized(&mut self, len: u32) -> Result<&'a [u8]> {
        self.take(usize::try_from(len).context("length does not fit this machine")?)
    }

    pub fn finish(self) -> Result<()> {
        if self.rest.is_empty() {
            return Ok(());
        }
        bail!("{} unexpected trailing bytes", self.rest.len())
    }
}

fn truncated(wanted: usize, available: usize) -> String {
    format!("truncated: wanted {wanted} bytes, {available} left")
}

#[cfg(test)]
mod tests {
    use super::Reader;

    #[test]
    fn reads_in_order() {
        let bytes = [1_u8, 0, 2, 0, 0, 0, 7, 8, 9];
        let mut reader = Reader::new(&bytes);

        assert_eq!(reader.u16().unwrap(), 1);
        assert_eq!(reader.u32().unwrap(), 2);
        assert_eq!(reader.take(3).unwrap(), [7, 8, 9]);
        assert!(reader.is_empty());
        reader.finish().unwrap();
    }

    #[test]
    fn refuses_to_read_past_the_end() {
        let bytes = [1_u8, 2];
        let mut reader = Reader::new(&bytes);

        let error = reader.u32().unwrap_err();

        assert!(error.to_string().contains("truncated"), "{error}");
    }

    #[test]
    fn refuses_a_length_it_cannot_honour() {
        let bytes = [1_u8, 2];
        let mut reader = Reader::new(&bytes);

        assert!(reader.sized(u32::MAX).is_err());
    }

    #[test]
    fn trailing_bytes_are_an_error() {
        let bytes = [1_u8, 2];
        let mut reader = Reader::new(&bytes);

        reader.u8().unwrap();

        assert!(reader.finish().is_err());
    }
}
