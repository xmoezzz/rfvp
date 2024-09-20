use std::collections::VecDeque;

use crate::time::{Ticks, Tween};

type Value = f32;

#[derive(Debug, Clone, Copy)]
enum State {
    Idle,
    Tweening {
        values: (Value, Value),
        time: Ticks,
        tween: Tween,
    },
}

/// Holds a value and plays back tweens which smoothly
/// adjust that value.
pub struct Tweener {
    tween_queue: VecDeque<(Value, Tween)>,
    state: State,
    value: Value,
}

impl Tweener {
    pub fn new(value: Value) -> Self {
        Self {
            tween_queue: VecDeque::new(),
            state: State::Idle,
            value,
        }
    }

    pub fn value(&self) -> Value {
        self.value
    }

    pub fn target_value(&self) -> Value {
        match self.state {
            State::Idle => self.value,
            State::Tweening {
                values: (_, value), ..
            } => match self.tween_queue.front() {
                None => value,
                Some(&(value, _)) => value,
            },
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.state, State::Idle)
    }

    /// Enqueues a new value to tween to.
    pub fn enqueue(&mut self, value: Value, tween: Tween) {
        match self.state {
            State::Idle => {
                self.state = State::Tweening {
                    values: (self.value, value),
                    time: Ticks::ZERO,
                    tween,
                };
            }
            State::Tweening { .. } => {
                self.tween_queue.push_back((value, tween));
            }
        }
    }

    /// Returns an linearly interpolated value between `a` and `b`.
    ///
    /// An amount of `0.0` should yield `a`, an amount of `1.0` should
    /// yield `b`, and an amount of `0.5` should yield a value halfway
    /// between `a` and `b`.
    fn lerp(a: Value, b: Value, amount: f32) -> Value {
        a + (b - a) * amount
    }

    fn next(&mut self, time: Ticks) {
        if let Some((value, tween)) = self.tween_queue.pop_front() {
            self.state = State::Tweening {
                values: (self.value, value),
                time,
                tween,
            };
        } else {
            self.state = State::Idle;
        }
    }

    pub fn update(&mut self, delta_time: Ticks) {
        if let State::Tweening {
            values,
            time,
            tween,
        } = &mut self.state
        {
            *time += delta_time;
            if *time >= tween.duration {
                self.value = values.1;
                let remaining_time = *time - tween.duration;
                self.next(remaining_time);
            } else {
                self.value = Self::lerp(values.0, values.1, tween.value(*time));
            }
        }
    }

    /// Fast-forwards the tweener to the last enqueue value.
    pub fn fast_forward(&mut self) {
        let last_queue_value = self.tween_queue.pop_front();
        self.tween_queue.clear();

        let value = match last_queue_value {
            Some((value, _)) => value,
            None => match self.state {
                State::Idle => self.value,
                State::Tweening { values, .. } => values.1,
            },
        };

        self.state = State::Idle;
        self.value = value;
    }

    /// Fast-forwards the tweener to the specified value.
    pub fn fast_forward_to(&mut self, value: Value) {
        self.tween_queue.clear();
        self.state = State::Idle;
        self.value = value;
    }

    /// Enqueue a transition from the current value to the specified value, ignoring the previous queue (it's cleared).
    pub fn enqueue_now(&mut self, value: Value, tween: Tween) {
        self.fast_forward_to(self.value);
        self.enqueue(value, tween);
    }
}
