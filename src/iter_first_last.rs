pub trait FirstLast: ExactSizeIterator {
    fn flag_first_last(self) -> FlagFirstLast<Self>
    where
        Self: Sized,
    {
        FlagFirstLast {
            starting_length: self.len(),
            iter: self,
        }
    }
}

impl<I: ExactSizeIterator> FirstLast for I {}

pub struct FlagFirstLast<I: ExactSizeIterator> {
    iter: I,
    starting_length: usize,
}

impl<I: ExactSizeIterator> Iterator for FlagFirstLast<I> {
    type Item = (bool, bool, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let is_first = self.iter.len() == self.starting_length;

        let item = self.iter.next()?;

        let is_last = self.iter.len() == 0;

        Some((is_first, is_last, item))
    }
}
