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

pub const ANIMATION_TICK_RATE_MS: u64 = 16;

impl EventHandler {
    pub fn new() -> Self {
        let tick_rate = Duration::from_millis(ANIMATION_TICK_RATE_MS);
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let sender_clone = sender.clone();
        let handler = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut tick = tokio::time::interval(tick_rate);
            const SCROLL_COOLDOWN_MS: u128 = 10;
            let mut last_scroll_time: Option<Instant> = None;
            loop {
                let tick_delay = tick.tick();
                let crossterm_event = reader.next().fuse();
                tokio::select! {
                    _ = sender_clone.closed() => break,
                    _ = tick_delay => {
                        sender_clone.send(Event::AnimationTick).unwrap();
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
