use crate::events;

struct Coord {
    x: usize,
    y: usize,
}

pub struct SnakeAnimation {
    body: Vec<Coord>,
    current_step: u64,
}

impl SnakeAnimation {
    pub fn new() -> Self {
        let mut snake = SnakeAnimation {
            body: Vec::new(),
            current_step: 0,
        };
        snake.add_segment(0, 0);
        snake.add_segment(0, 1);
        snake.add_segment(0, 2);
        snake
    }

    fn next_head_pos(&self) -> Coord {
        match self.body.last() {
            Some(head) => {
                match (head.x, head.y) {
                    (0, 0) => Coord { x: 0, y: 1 },
                    (0, 1) => Coord { x: 0, y: 2 },
                    (0, 2) => Coord { x: 0, y: 3 },
                    (0, 3) => Coord { x: 1, y: 3 },
                    (1, 3) => Coord { x: 1, y: 2 },
                    (1, 2) => Coord { x: 1, y: 1 },
                    (1, 1) => Coord { x: 1, y: 0 },
                    (1, 0) => Coord { x: 0, y: 0 },
                    _ => Coord { x: 0, y: 0 }, // should not happen
                }
            }
            None => Coord { x: 0, y: 0 },
        }
    }

    pub fn update_anim(&mut self, tick: u64) {
        let next_step: u64 = tick * events::ANIMATION_TICK_RATE_MS / 100;
        for step in self.current_step..next_step {
            let next_head = self.next_head_pos();

            self.add_segment(next_head.x, next_head.y);

            self.remove_tail();
        }
        self.current_step = next_step;
    }

    fn remove_tail(&mut self) {
        if !self.body.is_empty() {
            self.body.remove(0);
        }
    }

    fn add_segment(&mut self, x: usize, y: usize) {
        self.body.push(Coord { x, y });
    }

    fn body_as_grid(&self) -> Vec<[bool; 4]> {
        let mut grid: Vec<[bool; 4]> = vec![];
        for coord in self.body.iter() {
            if coord.x >= grid.len() {
                grid.resize(coord.x + 1, [false; 4]);
            }
            grid[coord.x][coord.y] = true;
        }

        grid
    }

    pub fn to_string(&self) -> String {
        let mut res = String::new();
        let grid = self.body_as_grid();
        for poss_col_pair in grid.chunks(2) {
            let col_pair = if poss_col_pair.len() % 2 == 1 {
                assert!(poss_col_pair.len() == 1);
                [poss_col_pair[0], [false; 4]]
            } else {
                [poss_col_pair[0], poss_col_pair[1]]
            };

            let ch = SnakeAnimation::unicode_char(
                col_pair[0][0],
                col_pair[1][0],
                col_pair[0][1],
                col_pair[1][1],
                col_pair[0][2],
                col_pair[1][2],
                col_pair[0][3],
                col_pair[1][3],
            );
            res.push(ch);
        }
        res
    }

    fn unicode_char(
        pos_0_0: bool,
        pos_0_1: bool,
        pos_1_0: bool,
        pos_1_1: bool,
        pos_2_0: bool,
        pos_2_1: bool,
        pos_3_0: bool,
        pos_3_1: bool,
    ) -> char {
        const BASE_CHAR: char = '⠀';
        let mut c = 0;
        c |= pos_0_0 as u32;
        c |= (pos_1_0 as u32) << 1;
        c |= (pos_2_0 as u32) << 2;
        c |= (pos_0_1 as u32) << 3;
        c |= (pos_1_1 as u32) << 4;
        c |= (pos_2_1 as u32) << 5;
        c |= (pos_3_0 as u32) << 6;
        c |= (pos_3_1 as u32) << 7;
        std::char::from_u32(BASE_CHAR as u32 + c).unwrap_or(BASE_CHAR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unicode_gen() {
        assert_eq!(
            SnakeAnimation::unicode_char(true, true, false, false, false, false, false, false),
            '⠉'
        );
        assert_eq!(
            SnakeAnimation::unicode_char(true, true, true, false, true, true, true, true),
            '⣯'
        );
        assert_eq!(
            SnakeAnimation::unicode_char(false, false, false, false, false, false, false, false),
            '⠀'
        );
    }
}
