use std::time::Instant;

use crate::unicode_helpers::{braille, BrailleDots};

struct Coord {
    x: usize,
    y: usize,
}

pub struct SnakeAnimation {
    body: Vec<Coord>,
    last_update_time: Instant,
}

impl SnakeAnimation {
    pub fn new() -> Self {
        let now = Instant::now();
        let mut snake = SnakeAnimation {
            body: Vec::new(),
            last_update_time: now,
        };
        snake.add_segment(0, 0);
        snake.add_segment(0, 1);
        snake.add_segment(0, 2);
        snake.add_segment(0, 3);
        snake
    }

    pub fn apply_to_string(&self, s: &str) -> String {
        let snake_chars: Vec<char> = self.to_string().chars().collect();

        s.chars()
            .enumerate()
            .map(|(i, original_char)| {
                snake_chars
                    .get(i)
                    .filter(|&&snake_char| snake_char != '⠀')
                    .unwrap_or(&original_char)
                    .to_owned()
            })
            .collect()
    }

    const MAX_X: usize = 12;
    const MAX_Y: usize = 4;

    fn num_steps_in_period() -> usize {
        Self::MAX_X * Self::MAX_Y
    }

    fn next_head_pos(&self) -> Coord {
        match self.body.last() {
            Some(head) => {
                match (head.x % 2, head.y) {
                    (0, 0) => Coord {
                        x: head.x % Self::MAX_X,
                        y: 1,
                    },
                    (0, 1) => Coord {
                        x: head.x % Self::MAX_X,
                        y: 2,
                    },
                    (0, 2) => Coord {
                        x: head.x % Self::MAX_X,
                        y: 3,
                    },
                    (0, 3) => Coord {
                        x: (head.x + 1) % Self::MAX_X,
                        y: 3,
                    },
                    (1, 3) => Coord {
                        x: head.x % Self::MAX_X,
                        y: 2,
                    },
                    (1, 2) => Coord {
                        x: head.x % Self::MAX_X,
                        y: 1,
                    },
                    (1, 1) => Coord {
                        x: head.x % Self::MAX_X,
                        y: 0,
                    },
                    (1, 0) => Coord {
                        x: (head.x + 1) % Self::MAX_X,
                        y: 0,
                    },
                    _ => Coord { x: 0, y: 0 }, // should not happen
                }
            }
            None => Coord { x: 0, y: 0 },
        }
    }

    pub fn update_anim(&mut self, now: Instant) {
        let elapsed_since_last = now.duration_since(self.last_update_time).as_secs_f32();

        // Calculate how many steps should have occurred (120ms per step)
        let steps_to_advance = (elapsed_since_last * 1000.0 / 120.0) as u64;
        let steps_to_advance = steps_to_advance as usize % Self::num_steps_in_period();

        for _ in 0..steps_to_advance {
            let next_head = self.next_head_pos();
            self.add_segment(next_head.x, next_head.y);
            self.remove_tail();
        }

        if steps_to_advance > 0 {
            self.last_update_time = now;
        }
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

    fn to_string(&self) -> String {
        let mut res = String::new();
        let grid = self.body_as_grid();
        for poss_col_pair in grid.chunks(2) {
            let col_pair = if poss_col_pair.len() % 2 == 1 {
                assert!(poss_col_pair.len() == 1);
                [poss_col_pair[0], [false; 4]]
            } else {
                [poss_col_pair[0], poss_col_pair[1]]
            };

            let ch = braille(BrailleDots(
                (col_pair[0][0] as u8)          // DOT_1 – top-left
                | ((col_pair[0][1] as u8) << 1) // DOT_2 – mid-left
                | ((col_pair[0][2] as u8) << 2) // DOT_3 – lower-left
                | ((col_pair[1][0] as u8) << 3) // DOT_4 – top-right
                | ((col_pair[1][1] as u8) << 4) // DOT_5 – mid-right
                | ((col_pair[1][2] as u8) << 5) // DOT_6 – lower-right
                | ((col_pair[0][3] as u8) << 6) // DOT_7 – bottom-left
                | ((col_pair[1][3] as u8) << 7), // DOT_8 – bottom-right
            ));
            res.push(ch);
        }
        res
    }

}

#[cfg(test)]
mod tests {
    use crate::unicode_helpers::{braille, BrailleDots};

    #[test]
    fn test_braille_top_row() {
        // DOT_1 (top-left) + DOT_4 (top-right) = bits 0 and 3 = 0x09 → U+2809 = '⠉'
        assert_eq!(braille(BrailleDots::DOT_1 | BrailleDots::DOT_4), '⠉');
    }

    #[test]
    fn test_braille_most_dots() {
        // All dots except DOT_5 (mid-right, bit 4) = 0xEF → U+28EF = '⣯'
        assert_eq!(
            braille(
                BrailleDots::DOT_1
                    | BrailleDots::DOT_2
                    | BrailleDots::DOT_3
                    | BrailleDots::DOT_4
                    | BrailleDots::DOT_6
                    | BrailleDots::DOT_7
                    | BrailleDots::DOT_8
            ),
            '⣯'
        );
    }

    #[test]
    fn test_braille_blank() {
        assert_eq!(braille(BrailleDots::EMPTY), '⠀');
    }
}
