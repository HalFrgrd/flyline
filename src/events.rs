use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, KeyEvent, MouseEvent};
use futures::{FutureExt, StreamExt};
use std::time::Instant;

#[derive(Clone, Debug)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    AnimationTick,
    Resize,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct EventHandler {
    pub sender: tokio::sync::mpsc::UnboundedSender<Event>,
    pub receiver: tokio::sync::mpsc::UnboundedReceiver<Event>,
    handler: tokio::task::JoinHandle<()>,
}

const ANIMATION_FPS_MAX: u64 = 60;
const ANIMATION_FPS_MIN: u64 = 5;
const ANIM_SWITCH_TIMEOUT_MS: u128 = 5000;

impl EventHandler {
    pub fn new() -> Self {
        let mut time_since_last_input = Instant::now();

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let sender_clone = sender.clone();
        let handler = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();

            let tick_rate = Duration::from_millis(1000 / ANIMATION_FPS_MAX);
            let mut tick = tokio::time::interval(tick_rate);

            const SCROLL_COOLDOWN_MS: u128 = 5;
            let mut last_scroll_time: Option<Instant> = None;
            loop {
                let tick_delay = tick.tick();
                let crossterm_event = reader.next().fuse();
                tokio::select! {
                    _ = sender_clone.closed() => break,
                    _ = tick_delay => {
                        sender_clone.send(Event::AnimationTick).unwrap();

                        let period = if time_since_last_input.elapsed().as_millis() < ANIM_SWITCH_TIMEOUT_MS {
                            Duration::from_millis(1000 / ANIMATION_FPS_MAX)
                        } else {
                            log::debug!("Switching to low FPS animation due to inactivity");
                            Duration::from_millis(1000 / ANIMATION_FPS_MIN)
                        };

                        tick = tokio::time::interval_at((Instant::now() + period).into(), period);


                    }
                    Some(Ok(evt)) = crossterm_event =>{
                        match evt {
                            CrosstermEvent::Key(key) => {
                                if key.kind == crossterm::event::KeyEventKind::Press {
                                  sender_clone.send(Event::Key(key)).unwrap();
                                }
                            }
                            CrosstermEvent::Mouse(mouse) => {
                                if mouse.kind == crossterm::event::MouseEventKind::ScrollDown || mouse.kind == crossterm::event::MouseEventKind::ScrollUp {
                                    if last_scroll_time.is_none() || last_scroll_time.unwrap().elapsed().as_millis() > SCROLL_COOLDOWN_MS {
                                        last_scroll_time = Some(Instant::now());
                                        sender_clone.send(Event::Mouse(mouse)).unwrap();
                                    }
                                } else {
                                    sender_clone.send(Event::Mouse(mouse)).unwrap();
                                }
                            }
                            CrosstermEvent::Resize(_, _) => {
                                sender_clone.send(Event::Resize).unwrap();
                            }
                            CrosstermEvent::FocusLost => {}
                            CrosstermEvent::FocusGained => {}
                            CrosstermEvent::Paste(_) => {}
                        }
                        time_since_last_input = Instant::now();

                    }
                }
            }
        });
        Self {
            sender,
            receiver,
            handler,
        }
    }
}
