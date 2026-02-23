use std::iter::Peekable;

pub trait FirstLast: Iterator {
    fn flag_first_last(self) -> FlagFirstLast<Self>
    where
        Self: Sized,
    {
        FlagFirstLast {
            idx: 0,
            iter: self.peekable(),
        }
    }
}

impl<I: Iterator> FirstLast for I {}

pub struct FlagFirstLast<I: Iterator> {
    iter: Peekable<I>,
    idx: usize,
}

impl<I: Iterator> Iterator for FlagFirstLast<I> {
    type Item = (bool, bool, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let is_first = self.idx == 0;

        let item = self.iter.next()?;
        self.idx += 1;

        let is_last = self.iter.peek().is_none();

        Some((is_first, is_last, item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_first_last() {
        let v = vec!['a', 'b', 'c'];
        let flagged: Vec<(bool, bool, char)> = v.into_iter().flag_first_last().collect();
        assert_eq!(
            flagged,
            vec![(true, false, 'a'), (false, false, 'b'), (false, true, 'c')]
        );
    }

    #[test]
    fn test_flag_first_last_single_element() {
        let v = vec!['x'];
        let flagged: Vec<(bool, bool, char)> = v.into_iter().flag_first_last().collect();
        assert_eq!(flagged, vec![(true, true, 'x')]);
    }
}
