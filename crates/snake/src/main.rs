/// A terminal snake game.
///
/// Built to verify the project toolchain works end-to-end.
/// Uses termion for raw-keyboard input and terminal control.
///
/// Controls: Arrow keys to steer, 'q' to quit.
/// Eat the '*' food to grow. Don't hit walls or yourself.
use std::io::{stdin, stdout, Write};
use std::time::Duration;

use rand::Rng;
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::{clear, cursor};

/// Game world dimensions.
const WIDTH: u16 = 30;
const HEIGHT: u16 = 20;

/// Direction the snake is moving.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// A single (x, y) coordinate on the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point(u16, u16);

/// The snake game state.
struct Snake {
    /// Body segments: head is index 0.
    body: Vec<Point>,
    /// Current movement direction.
    dir: Direction,
    /// Next direction queued by the player (applied each tick).
    next_dir: Direction,
    /// The food position.
    food: Point,
    /// Current score.
    score: u32,
    /// Whether the game is over.
    game_over: bool,
}

impl Snake {
    fn new() -> Self {
        let mut rng = rand::thread_rng();
        // Snake starts at the center, moving right, length 3.
        let cx = WIDTH / 2;
        let cy = HEIGHT / 2;
        let body = vec![
            Point(cx, cy),
            Point(cx - 1, cy),
            Point(cx - 2, cy),
        ];
        let food = spawn_food(&body, &mut rng);
        Snake {
            body,
            dir: Direction::Right,
            next_dir: Direction::Right,
            food,
            score: 0,
            game_over: false,
        }
    }

    /// Change direction (no reversing).
    fn turn(&mut self, d: Direction) {
        let invalid = match (&self.dir, &d) {
            (Direction::Up, Direction::Down) => true,
            (Direction::Down, Direction::Up) => true,
            (Direction::Left, Direction::Right) => true,
            (Direction::Right, Direction::Left) => true,
            _ => false,
        };
        if !invalid {
            self.next_dir = d;
        }
    }

    /// Advance one tick.
    fn tick(&mut self) {
        if self.game_over {
            return;
        }
        self.dir = self.next_dir;
        let head = self.body[0];
        let new_head = match self.dir {
            Direction::Up => Point(head.0, head.1.wrapping_sub(1)),
            Direction::Down => Point(head.0, head.1 + 1),
            Direction::Left => Point(head.0.wrapping_sub(1), head.1),
            Direction::Right => Point(head.0 + 1, head.1),
        };

        // Check wall collision.
        if new_head.0 >= WIDTH || new_head.1 >= HEIGHT {
            self.game_over = true;
            return;
        }

        // Check self collision.
        if self.body.contains(&new_head) {
            self.game_over = true;
            return;
        }

        // Move: insert new head, remove tail (unless eating food).
        self.body.insert(0, new_head);
        if new_head == self.food {
            self.score += 10;
            let mut rng = rand::thread_rng();
            self.food = spawn_food(&self.body, &mut rng);
        } else {
            self.body.pop();
        }
    }

    /// Render the current frame to a string.
    fn render(&self) -> String {
        let mut s = String::new();
        // Title row.
        s.push_str(&format!("{}Ante Snake — Score: {}\r\n", clear::All, self.score));

        // Top border.
        s.push_str(&format!("+{}+", "-".repeat(WIDTH as usize)));
        s.push_str("\r\n");

        for y in 0..HEIGHT {
            s.push('|');
            for x in 0..WIDTH {
                let p = Point(x, y);
                if p == self.food {
                    s.push('*');
                } else if self.body[0] == p {
                    s.push('@');
                } else if self.body.contains(&p) {
                    s.push('o');
                } else {
                    s.push(' ');
                }
            }
            s.push('|');
            s.push_str("\r\n");
        }

        // Bottom border.
        s.push_str(&format!("+{}+", "-".repeat(WIDTH as usize)));
        s.push_str("\r\n");

        if self.game_over {
            s.push_str(&format!("\r\nGame Over! Final score: {}\r\n", self.score));
            s.push_str("Press 'q' to quit.\r\n");
        } else {
            s.push_str("\r\nArrow keys: steer   q: quit\r\n");
        }
        s
    }
}

/// Spawn food at a random position that isn't occupied.
fn spawn_food(body: &[Point], rng: &mut impl Rng) -> Point {
    loop {
        let p = Point(rng.gen_range(0..WIDTH), rng.gen_range(0..HEIGHT));
        if !body.contains(&p) {
            return p;
        }
    }
}

fn main() {
    let stdout = stdout();
    let mut stdout = stdout.lock().into_raw_mode().unwrap();
    let mut stdin = stdin().keys();

    write!(stdout, "{}{}", clear::All, cursor::Goto(1, 1)).unwrap();
    stdout.flush().unwrap();

    let mut game = Snake::new();

    // Draw initial frame.
    write!(
        stdout,
        "{}{}",
        cursor::Goto(1, 1),
        game.render()
    )
    .unwrap();
    stdout.flush().unwrap();

    'game_loop: loop {
        // Wait for input with a short tick timeout (~150ms).
        let key = stdin.next();

        match key {
            Some(Ok(Key::Char('q'))) => break 'game_loop,
            Some(Ok(Key::Up)) => game.turn(Direction::Up),
            Some(Ok(Key::Down)) => game.turn(Direction::Down),
            Some(Ok(Key::Left)) => game.turn(Direction::Left),
            Some(Ok(Key::Right)) => game.turn(Direction::Right),
            _ => {}
        }

        if game.game_over {
            // Still allow steering while game over? No — just wait for 'q'.
            // But we need to re-draw in case of a stray key press that got consumed.
            write!(
                stdout,
                "{}{}",
                cursor::Goto(1, 1),
                game.render()
            )
            .unwrap();
            stdout.flush().unwrap();

            // Wait for 'q' to quit.
            loop {
                match stdin.next() {
                    Some(Ok(Key::Char('q'))) => break 'game_loop,
                    _ => continue,
                }
            }
        }

        game.tick();

        // Move cursor to top and redraw.
        write!(
            stdout,
            "{}{}",
            cursor::Goto(1, 1),
            game.render()
        )
        .unwrap();
        stdout.flush().unwrap();

        // Brief sleep so the game runs at a readable pace.
        std::thread::sleep(Duration::from_millis(150));
    }

    // Restore terminal.
    write!(stdout, "{}{}", clear::All, cursor::Goto(1, 1)).unwrap();
    stdout.flush().unwrap();
    writeln!(stdout, "Thanks for playing Ante Snake!").unwrap();
}
