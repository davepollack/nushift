use druid::widget::prelude::*;
use druid::{Data, KeyOrValue, Color};
use druid::kurbo::{CubicBez, ParamCurve};

#[derive(Debug, Clone)]
enum TransitionDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
enum TransitionState {
    Transitioning(f64, TransitionDirection),
    Stopped(bool),
}

pub fn default_easing_function() -> impl Fn(f64) -> f64 {
    let default_ease_cubic_bez = CubicBez::new(
        (0., 0.), (0.25, 0.1), (0.25, 1.0), (1.0, 1.0)
    );
    move |t| default_ease_cubic_bez.eval(t).y
}

/// A widget that wraps an inner widget and provides an animating background on
/// hover.
pub struct HoverBackground<T> {
    color: KeyOrValue<Color>,
    min_alpha: f64,
    max_alpha: f64,
    active_color: KeyOrValue<Color>,
    easing_function_in: Box<dyn FnMut(f64) -> f64 + 'static>,
    easing_function_out: Box<dyn FnMut(f64) -> f64 + 'static>,
    duration: f64,
    inner: Box<dyn Widget<T>>,

    transition_state: TransitionState,
}

impl<T: Data> HoverBackground<T> {
    /// Create a new widget to provide a hover background, given colour and
    /// timing information.
    ///
    /// The `color` argument can be either a concrete `Color`, or a Druid `Key`
    /// resolvable in the `Env`.
    ///
    /// The `easing_function`s will be called with an `f64` between `0.0` and
    /// `1.0` representing time, and they should return an `f64` between `0.0`
    /// and `1.0` representing the progress of the animation.
    pub fn new(
        color: impl Into<KeyOrValue<Color>>,
        min_alpha: impl Into<f64>,
        max_alpha: impl Into<f64>,
        active_color: impl Into<KeyOrValue<Color>>,
        easing_function_in: impl FnMut(f64) -> f64 + 'static,
        easing_function_out: impl FnMut(f64) -> f64 + 'static,
        duration: impl Into<f64>,
        inner: impl Widget<T> + 'static,
    ) -> Self {
        Self {
            color: color.into(),
            min_alpha: min_alpha.into(),
            max_alpha: max_alpha.into(),
            active_color: active_color.into(),
            easing_function_in: Box::new(easing_function_in),
            easing_function_out: Box::new(easing_function_out),
            duration: duration.into(),
            inner: Box::new(inner),

            transition_state: TransitionState::Stopped(false),
        }
    }

    fn animate_forward(&mut self) {
        match self.transition_state {
            TransitionState::Transitioning(_, TransitionDirection::Forward) => {},
            TransitionState::Transitioning(t, TransitionDirection::Backward) => {
                self.transition_state = TransitionState::Transitioning(t, TransitionDirection::Forward);
            },
            TransitionState::Stopped(true) => {}
            TransitionState::Stopped(false) => {
                self.transition_state = TransitionState::Transitioning(0.0, TransitionDirection::Forward);
            }
        }
    }

    fn animate_backward(&mut self) {
        match self.transition_state {
            TransitionState::Transitioning(t, TransitionDirection::Forward) => {
                self.transition_state = TransitionState::Transitioning(t, TransitionDirection::Backward);
            },
            TransitionState::Transitioning(_, TransitionDirection::Backward) => {},
            TransitionState::Stopped(true) => {
                self.transition_state = TransitionState::Transitioning(1.0, TransitionDirection::Backward);
            }
            TransitionState::Stopped(false) => {}
        }
    }

    fn update_animation_state(&mut self, interval_nanoseconds: &u64) -> bool {
        let interval_seconds = *interval_nanoseconds as f64 / 1e9;

        let (new_state, should_request_anim_frame) = match &self.transition_state {
            TransitionState::Transitioning(t, TransitionDirection::Forward) => {
                let new_t = (1.0 as f64).min(t + (interval_seconds / self.duration));
                if new_t >= 1.0 {
                    (TransitionState::Stopped(true), false)
                } else {
                    (TransitionState::Transitioning(new_t, TransitionDirection::Forward), true)
                }
            },
            TransitionState::Transitioning(t, TransitionDirection::Backward) => {
                let new_t = (0.0 as f64).max(t - (interval_seconds / self.duration));
                if new_t <= 0.0 {
                    (TransitionState::Stopped(false), false)
                } else {
                    (TransitionState::Transitioning(new_t, TransitionDirection::Backward), true)
                }
            },
            stopped_state => (stopped_state.clone(), false),
        };

        self.transition_state = new_state;

        should_request_anim_frame
    }
}

impl<T: Data> Widget<T> for HoverBackground<T> {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        self.inner.event(ctx, event, data, env);
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &T, env: &Env) {
        match event {
            LifeCycle::HotChanged(true) => {
                ctx.request_paint();
                self.animate_forward();
                ctx.request_anim_frame();
            },
            LifeCycle::HotChanged(false) => {
                ctx.request_paint();
                self.animate_backward();
                ctx.request_anim_frame();
            },
            LifeCycle::FocusChanged(_) => ctx.request_paint(),
            LifeCycle::AnimFrame(interval) => {
                ctx.request_paint();
                let should_request_anim_frame = self.update_animation_state(interval);
                if should_request_anim_frame {
                    ctx.request_anim_frame();
                }
            },
            _ => {},
        }

        self.inner.lifecycle(ctx, event, data, env);
    }

    fn update(&mut self, ctx: &mut UpdateCtx, old_data: &T, data: &T, env: &Env) {
        self.inner.update(ctx, old_data, data, env);
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, data: &T, env: &Env) -> Size {
        self.inner.layout(ctx, bc, data, env)
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &T, env: &Env) {
        let color = if ctx.is_active() {
            self.active_color.resolve(env)
        } else {
            let alpha = match self.transition_state {
                TransitionState::Transitioning(t, TransitionDirection::Forward) => {
                    self.min_alpha + ((self.easing_function_in)(t) * (self.max_alpha - self.min_alpha))
                },
                TransitionState::Transitioning(t, TransitionDirection::Backward) => {
                    self.min_alpha + ((self.easing_function_out)(t) * (self.max_alpha - self.min_alpha))
                },
                TransitionState::Stopped(false) => self.min_alpha,
                TransitionState::Stopped(true) => self.max_alpha,
            };
            self.color.resolve(env).with_alpha(alpha)
        };
        let bounds = ctx.size().to_rect();
        ctx.fill(bounds, &color);
        self.inner.paint(ctx, data, env);
    }

    fn id(&self) -> Option<WidgetId> {
        self.inner.id()
    }
}

// TODO add tests
