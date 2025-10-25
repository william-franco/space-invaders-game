use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
};
use std::error::Error;
use std::io;
use std::time::{Duration, Instant};

// Basic position struct for any entity (player, bullet, enemy)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pos {
    x: u16,
    y: u16,
}

// Game configuration parameters
struct GameConfig {
    tick_ms: u64,
    initial_enemy_rows: usize,
    initial_enemy_cols: usize,
    enemy_move_every_ticks: u64,
    enemy_speedup_every_kills: usize,
}

// Holds all dynamic game state
struct GameState {
    width: u16,
    height: u16,
    player: Pos,
    bullets: Vec<Pos>,
    enemies: Vec<Pos>,
    score: usize,
    kills: usize,
    tick_count: u64,
    enemy_tick_acc: u64,
    enemy_move_every_ticks: u64,
    enemy_direction: i8,
    game_over: bool,
    victory: bool,
    spawn_rows: usize,
    spawn_cols: usize,
    level: usize,
}

impl GameState {
    // Initialize new game
    fn new(width: u16, height: u16, cfg: &GameConfig) -> Self {
        let player = Pos {
            x: width / 2,
            y: height - 3,
        };
        let mut gs = GameState {
            width,
            height,
            player,
            bullets: Vec::new(),
            enemies: Vec::new(),
            score: 0,
            kills: 0,
            tick_count: 0,
            enemy_tick_acc: 0,
            enemy_move_every_ticks: cfg.enemy_move_every_ticks,
            enemy_direction: 1,
            game_over: false,
            victory: false,
            spawn_rows: cfg.initial_enemy_rows,
            spawn_cols: cfg.initial_enemy_cols,
            level: 1,
        };
        gs.spawn_enemies();
        gs
    }

    // Generate a grid of enemies at the top
    fn spawn_enemies(&mut self) {
        self.enemies.clear();
        let left_margin = 2;
        let right_margin = 2;
        let usable_w = self.width.saturating_sub(left_margin + right_margin);
        let cols = self.spawn_cols as u16;
        let spacing_x = (usable_w / (cols + 1)).max(1);

        for row in 0..self.spawn_rows as u16 {
            for col in 0..cols {
                let x = left_margin + spacing_x * (col + 1);
                let y = 2 + row * 2;
                if x < self.width - 1 && y < self.height - 2 {
                    self.enemies.push(Pos { x, y });
                }
            }
        }
    }

    // Reset state for restart
    fn reset(&mut self, cfg: &GameConfig) {
        self.player = Pos {
            x: self.width / 2,
            y: self.height - 3,
        };
        self.bullets.clear();
        self.enemies.clear();
        self.score = 0;
        self.kills = 0;
        self.tick_count = 0;
        self.enemy_tick_acc = 0;
        self.enemy_move_every_ticks = cfg.enemy_move_every_ticks;
        self.enemy_direction = 1;
        self.game_over = false;
        self.victory = false;
        self.spawn_rows = cfg.initial_enemy_rows;
        self.spawn_cols = cfg.initial_enemy_cols;
        self.level = 1;
        self.spawn_enemies();
    }

    // Update all entities and handle game logic each tick
    fn tick(&mut self, _cfg: &GameConfig) {
        if self.game_over || self.victory {
            return;
        }

        self.tick_count += 1;
        self.enemy_tick_acc += 1;

        // Move bullets up
        for b in self.bullets.iter_mut() {
            if b.y > 0 {
                b.y -= 1;
            }
        }
        self.bullets.retain(|b| b.y > 0);

        // Detect bullet-enemy collisions
        let mut to_remove = Vec::new();
        for b in &self.bullets {
            if let Some(ei) = self.enemies.iter().position(|e| e.x == b.x && e.y == b.y) {
                to_remove.push(ei);
                self.score += 10;
                self.kills += 1;
            }
        }
        to_remove.sort_unstable();
        to_remove.dedup();
        for idx in to_remove.iter().rev() {
            if *idx < self.enemies.len() {
                self.enemies.remove(*idx);
            }
        }

        // Level up when all enemies are gone
        if self.enemies.is_empty() {
            self.level += 1;
            if self.level % 2 == 0 {
                self.spawn_rows = (self.spawn_rows + 1).min(6);
            } else {
                self.spawn_cols = (self.spawn_cols + 1).min(12);
            }
            self.enemy_move_every_ticks = self.enemy_move_every_ticks.saturating_sub(1).max(1);
            self.spawn_enemies();
        }

        // Move enemies horizontally and down
        if self.enemy_tick_acc >= self.enemy_move_every_ticks {
            self.enemy_tick_acc = 0;
            let shift = self.enemy_direction as i16;
            let hit_side = self
                .enemies
                .iter()
                .any(|e| e.x as i16 + shift <= 1 || e.x as i16 + shift >= (self.width as i16 - 2));

            if hit_side {
                // move down and reverse direction
                for e in &mut self.enemies {
                    e.y += 1;
                }
                self.enemy_direction *= -1;
            } else {
                for e in &mut self.enemies {
                    e.x = (e.x as i16 + shift) as u16;
                }
            }
        }

        // Check if enemies reached bottom
        if self.enemies.iter().any(|e| e.y >= self.player.y) {
            self.game_over = true;
        }
    }

    // Player shooting
    fn shoot(&mut self) {
        if self.bullets.len() < 3 {
            self.bullets.push(Pos {
                x: self.player.x,
                y: self.player.y.saturating_sub(1),
            });
        }
    }

    // Player movement
    fn move_player_left(&mut self) {
        if self.player.x > 1 {
            self.player.x -= 1;
        }
    }
    fn move_player_right(&mut self) {
        if self.player.x < self.width.saturating_sub(2) {
            self.player.x += 1;
        }
    }

    fn enemies_remaining(&self) -> usize {
        self.enemies.len()
    }

    // Progress indicator (for the info panel)
    fn progress(&self) -> f64 {
        let total_expected = (self.spawn_rows * self.spawn_cols).max(1) + (self.level - 1) * 2;
        (self.kills as f64 / total_expected as f64).min(1.0)
    }
}

// Draw the main play area
fn draw_game<B: ratatui::backend::Backend>(f: &mut ratatui::Frame<B>, area: Rect, gs: &GameState) {
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        format!(" Space Invaders - Level {} ", gs.level),
        Style::default()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    // Prepare 2D char grid for rendering entities
    let mut grid = vec![vec![(' ', Style::default()); inner.width as usize]; inner.height as usize];

    // Draw enemies
    for e in &gs.enemies {
        if e.x >= inner.x && e.y >= inner.y {
            let lx = e.x - inner.x;
            let ly = e.y - inner.y;
            if lx < inner.width && ly < inner.height {
                grid[ly as usize][lx as usize] = (
                    '#',
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                );
            }
        }
    }

    // Draw bullets
    for b in &gs.bullets {
        if b.x >= inner.x && b.y >= inner.y {
            let lx = b.x - inner.x;
            let ly = b.y - inner.y;
            if lx < inner.width && ly < inner.height {
                grid[ly as usize][lx as usize] = (
                    '|',
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                );
            }
        }
    }

    // Draw player
    let p = &gs.player;
    if p.x >= inner.x && p.y >= inner.y {
        let lx = p.x - inner.x;
        let ly = p.y - inner.y;
        if lx < inner.width && ly < inner.height {
            grid[ly as usize][lx as usize] = (
                '^',
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }

    // Convert grid to styled text for ratatui Paragraph
    let spans: Vec<Line> = grid
        .iter()
        .map(|row| {
            Line::from(
                row.iter()
                    .map(|(c, s)| Span::styled(c.to_string(), *s))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    let play = Paragraph::new(spans).wrap(Wrap { trim: false });
    f.render_widget(play, inner);
}

// Draw score, info panel, progress bar, etc.
fn draw_ui<B: ratatui::backend::Backend>(f: &mut ratatui::Frame<B>, gs: &GameState) {
    let size = f.size();

    // Split screen into header and main section
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10)].as_ref())
        .split(size);

    // Header: score and controls
    let score_text = Line::from(vec![
        Span::raw(" Score: "),
        Span::styled(gs.score.to_string(), Style::default().fg(Color::Yellow)),
        Span::raw("  Enemies: "),
        Span::styled(
            gs.enemies_remaining().to_string(),
            Style::default().fg(Color::Red),
        ),
        Span::raw("  Level: "),
        Span::styled(gs.level.to_string(), Style::default().fg(Color::Green)),
        Span::raw("  (q: quit, space: shoot, a/d or ←/→: move)"),
    ]);
    let header =
        Paragraph::new(score_text).block(Block::default().borders(Borders::ALL).title(" Status "));
    f.render_widget(header, chunks[0]);

    // Split bottom area: left = game, right = info
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)].as_ref())
        .split(chunks[1]);

    draw_game(f, bottom[0], gs);

    // Info panel with progress bar
    let info_block = Block::default().borders(Borders::ALL).title(" Info ");
    f.render_widget(info_block, bottom[1]);

    let inner = Rect {
        x: bottom[1].x + 1,
        y: bottom[1].y + 1,
        width: bottom[1].width.saturating_sub(2),
        height: bottom[1].height.saturating_sub(2),
    };

    let g = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(gs.progress());
    f.render_widget(g, inner);

    // Show game over / win overlay
    if gs.game_over || gs.victory {
        let msg = if gs.victory { "YOU WIN!" } else { "GAME OVER" };
        let rect = Rect {
            x: size.x + (size.width / 2) - 15,
            y: size.y + (size.height / 2) - 3,
            width: 30,
            height: 6,
        };
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            msg,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        f.render_widget(block, rect);
        let info = Paragraph::new(vec![
            Line::from(format!("Final score: {}", gs.score)),
            Line::from("Press 'r' to restart or 'q' to quit."),
        ]);
        f.render_widget(
            info,
            Rect {
                x: rect.x + 1,
                y: rect.y + 2,
                width: rect.width - 2,
                height: 3,
            },
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Configure base game settings
    let cfg = GameConfig {
        tick_ms: 100,
        initial_enemy_rows: 3,
        initial_enemy_cols: 6,
        enemy_move_every_ticks: 6,
        enemy_speedup_every_kills: 5,
    };

    // Setup terminal in raw + alternate screen mode
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let size = terminal.size()?;
    let mut gs = GameState::new(size.width, size.height, &cfg);

    let tick_rate = Duration::from_millis(cfg.tick_ms);
    let mut last_tick = Instant::now();

    // Main event loop
    loop {
        terminal.draw(|f| draw_ui(f, &gs))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_millis(0));

        // Handle keyboard and resize events
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(KeyEvent {
                    code, modifiers, ..
                }) => match code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('a') | KeyCode::Left => gs.move_player_left(),
                    KeyCode::Char('d') | KeyCode::Right => gs.move_player_right(),
                    KeyCode::Char('r') => {
                        if gs.game_over || gs.victory {
                            gs.reset(&cfg);
                        }
                    }
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        if !gs.game_over && !gs.victory {
                            gs.shoot();
                        }
                    }
                    KeyCode::Char('c') if modifiers == KeyModifiers::CONTROL => break,
                    _ => {}
                },
                Event::Resize(w, h) => {
                    gs.width = w;
                    gs.height = h;
                    gs.player.y = gs.height.saturating_sub(3);
                }
                _ => {}
            }
        }

        // Tick game logic at fixed interval
        if last_tick.elapsed() >= tick_rate {
            gs.tick(&cfg);
            if gs.kills > 0 && gs.kills % cfg.enemy_speedup_every_kills == 0 {
                gs.enemy_move_every_ticks = gs.enemy_move_every_ticks.saturating_sub(1).max(1);
            }
            if gs.enemies.is_empty() {
                gs.victory = true;
            }
            last_tick = Instant::now();
        }
    }

    // Restore terminal before exiting
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    println!("Thanks for playing! Final score: {}", gs.score);
    Ok(())
}
