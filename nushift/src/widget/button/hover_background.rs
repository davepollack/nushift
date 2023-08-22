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

pub fn ease_out() -> impl Fn(f64) -> f64 {
    let ease_out_cubic_bez = CubicBez::new(
        (0., 0.), (0., 0.), (0.58, 1.0), (1.0, 1.0)
    );
    move |t| ease_out_cubic_bez.eval(t).y
}

pub fn ease_in() -> impl Fn(f64) -> f64 {
    let ease_in_cubic_bez = CubicBez::new(
        (0., 0.), (0.42, 0.), (1.0, 1.0), (1.0, 1.0)
    );
    move |t| ease_in_cubic_bez.eval(t).y
}

pub struct HoverParams {
    color: KeyOrValue<Color>,
    min_alpha: f64,
    max_alpha: f64,
    active_color: KeyOrValue<Color>,
    easing_function_in: Box<dyn FnMut(f64) -> f64 + 'static>,
    easing_function_out: Box<dyn FnMut(f64) -> f64 + 'static>,
    duration: f64,
}

impl HoverParams {
    /// Create a new `HoverParams` with colour and timing information.
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
    ) -> Self {
        Self {
            color: color.into(),
            min_alpha: min_alpha.into(),
            max_alpha: max_alpha.into(),
            active_color: active_color.into(),
            easing_function_in: Box::new(easing_function_in),
            easing_function_out: Box::new(easing_function_out),
            duration: duration.into(),
        }
    }
}

impl Default for HoverParams {
    fn default() -> Self {
        HoverParams::new(
            Color::grey(0.0), 0.0, 0.1,
            Color::grey(0.0).with_alpha(0.25),
            ease_out(), ease_in(),
            0.07,
        )
    }
}

/// A widget that wraps an inner widget and provides an animating background on
/// hover.
pub struct HoverBackground<T> {
    inner: Box<dyn Widget<T>>,
    params: HoverParams,
    transition_state: TransitionState,
}

impl<T: Data> HoverBackground<T> {
    pub fn new(inner: impl Widget<T> + 'static, params: impl Into<HoverParams>) -> Self {
        Self {
            inner: Box::new(inner),
            params: params.into(),
            transition_state: TransitionState::Stopped(false),
        }
    }

    fn animate_forward(&mut self) {
        let new_state = match self.transition_state {
            TransitionState::Transitioning(_, TransitionDirection::Forward) => self.transition_state.clone(),
            TransitionState::Transitioning(t, TransitionDirection::Backward) => TransitionState::Transitioning(t, TransitionDirection::Forward),
            TransitionState::Stopped(false) => TransitionState::Transitioning(0.0, TransitionDirection::Forward),
            TransitionState::Stopped(true) => self.transition_state.clone(),
        };
        self.transition_state = new_state;
    }

    fn animate_backward(&mut self) {
        let new_state = match self.transition_state {
            TransitionState::Transitioning(t, TransitionDirection::Forward) => TransitionState::Transitioning(t, TransitionDirection::Backward),
            TransitionState::Transitioning(_, TransitionDirection::Backward) => self.transition_state.clone(),
            TransitionState::Stopped(false) => self.transition_state.clone(),
            TransitionState::Stopped(true) => TransitionState::Transitioning(1.0, TransitionDirection::Backward),
        };
        self.transition_state = new_state;
    }

    fn advance_animation(&mut self, interval_nanoseconds: &u64) -> bool {
        let interval_seconds = *interval_nanoseconds as f64 / 1e9;

        let (new_state, should_request_anim_frame) = match self.transition_state {
            TransitionState::Transitioning(t, TransitionDirection::Forward) => {
                let new_t = (1.0 as f64).min(t + (interval_seconds / self.params.duration));
                if new_t >= 1.0 {
                    (TransitionState::Stopped(true), false)
                } else {
                    (TransitionState::Transitioning(new_t, TransitionDirection::Forward), true)
                }
            },
            TransitionState::Transitioning(t, TransitionDirection::Backward) => {
                let new_t = (0.0 as f64).max(t - (interval_seconds / self.params.duration));
                if new_t <= 0.0 {
                    (TransitionState::Stopped(false), false)
                } else {
                    (TransitionState::Transitioning(new_t, TransitionDirection::Backward), true)
                }
            },
            ref stopped_state => (stopped_state.clone(), false),
        };

        self.transition_state = new_state;

        should_request_anim_frame
    }
}

impl<T: Data> Widget<T> for HoverBackground<T> {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        match event {
            Event::AnimFrame(interval) => {
                ctx.request_paint();
                let should_request_anim_frame = self.advance_animation(interval);
                if should_request_anim_frame {
                    ctx.request_anim_frame();
                }
            },
            _ => {},
        }

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
            self.params.active_color.resolve(env)
        } else {
            let alpha = match self.transition_state {
                TransitionState::Transitioning(t, TransitionDirection::Forward) => {
                    self.params.min_alpha + ((self.params.easing_function_in)(t) * (self.params.max_alpha - self.params.min_alpha))
                },
                TransitionState::Transitioning(t, TransitionDirection::Backward) => {
                    self.params.min_alpha + ((self.params.easing_function_out)(t) * (self.params.max_alpha - self.params.min_alpha))
                },
                TransitionState::Stopped(false) => self.params.min_alpha,
                TransitionState::Stopped(true) => self.params.max_alpha,
            };
            self.params.color.resolve(env).with_alpha(alpha)
        };
        let bounds = ctx.size().to_rect();
        ctx.fill(bounds, &color);
        self.inner.paint(ctx, data, env);
    }

    fn id(&self) -> Option<WidgetId> {
        self.inner.id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use druid::widget::SizedBox;

    fn create() -> HoverBackground<()> {
        HoverBackground::new(SizedBox::empty(), HoverParams::default())
    }

    mod animate_forward {
        use super::*;

        #[test]
        fn if_already_forward_then_do_nothing() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Transitioning(0.5, TransitionDirection::Forward);
            hover_background.animate_forward();
            assert!(matches!(hover_background.transition_state, TransitionState::Transitioning(_, TransitionDirection::Forward)));
        }

        #[test]
        fn if_backward_then_change_to_forward() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Transitioning(0.5, TransitionDirection::Backward);
            hover_background.animate_forward();
            assert!(matches!(hover_background.transition_state, TransitionState::Transitioning(_, TransitionDirection::Forward)));
        }

        #[test]
        fn if_stopped_at_beginning_then_change_to_forward() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Stopped(false);
            hover_background.animate_forward();
            assert!(matches!(hover_background.transition_state, TransitionState::Transitioning(_, TransitionDirection::Forward)));
        }

        #[test]
        fn if_stopped_at_end_then_do_nothing() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Stopped(true);
            hover_background.animate_forward();
            assert!(matches!(hover_background.transition_state, TransitionState::Stopped(true)));
        }
    }

    mod animate_backward {
        use super::*;

        #[test]
        fn if_forward_then_change_to_backward() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Transitioning(0.5, TransitionDirection::Forward);
            hover_background.animate_backward();
            assert!(matches!(hover_background.transition_state, TransitionState::Transitioning(_, TransitionDirection::Backward)));
        }

        #[test]
        fn if_already_backward_then_do_nothing() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Transitioning(0.5, TransitionDirection::Backward);
            hover_background.animate_backward();
            assert!(matches!(hover_background.transition_state, TransitionState::Transitioning(_, TransitionDirection::Backward)));
        }

        #[test]
        fn if_stopped_at_beginning_then_do_nothing() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Stopped(false);
            hover_background.animate_backward();
            assert!(matches!(hover_background.transition_state, TransitionState::Stopped(false)));
        }

        #[test]
        fn if_stopped_at_end_then_change_to_backward() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Stopped(true);
            hover_background.animate_backward();
            assert!(matches!(hover_background.transition_state, TransitionState::Transitioning(_, TransitionDirection::Backward)));
        }
    }

    mod advance_animation {
        use super::*;

        const ADVANCEMENT_AMOUNT_NANOSECONDS: u64 = 35_000_000; // Half of default duration

        #[test]
        fn if_currently_going_forward_then_advance_further() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Transitioning(0.2, TransitionDirection::Forward);

            let should_request_anim_frame = hover_background.advance_animation(&ADVANCEMENT_AMOUNT_NANOSECONDS);

            assert!(match hover_background.transition_state {
                TransitionState::Transitioning(t, TransitionDirection::Forward) => t > 0.6 && t < 0.8,
                _ => false,
            });
            assert!(should_request_anim_frame);
        }

        #[test]
        fn if_currently_going_forward_and_going_over_limit_then_stop() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Transitioning(0.8, TransitionDirection::Forward);

            let should_request_anim_frame = hover_background.advance_animation(&ADVANCEMENT_AMOUNT_NANOSECONDS);

            assert!(matches!(hover_background.transition_state, TransitionState::Stopped(true)));
            assert!(!should_request_anim_frame);
        }

        #[test]
        fn if_currently_going_backward_then_advance_further() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Transitioning(0.8, TransitionDirection::Backward);

            let should_request_anim_frame = hover_background.advance_animation(&ADVANCEMENT_AMOUNT_NANOSECONDS);

            assert!(match hover_background.transition_state {
                TransitionState::Transitioning(t, TransitionDirection::Backward) => t > 0.2 && t < 0.4,
                _ => false,
            });
            assert!(should_request_anim_frame);
        }

        #[test]
        fn if_currently_going_backward_and_going_over_limit_then_stop() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Transitioning(0.2, TransitionDirection::Backward);

            let should_request_anim_frame = hover_background.advance_animation(&ADVANCEMENT_AMOUNT_NANOSECONDS);

            assert!(matches!(hover_background.transition_state, TransitionState::Stopped(false)));
            assert!(!should_request_anim_frame);
        }

        #[test]
        fn if_stopped_then_do_nothing() {
            let mut hover_background = create();
            hover_background.transition_state = TransitionState::Stopped(false);

            let mut should_request_anim_frame = hover_background.advance_animation(&ADVANCEMENT_AMOUNT_NANOSECONDS);

            assert!(matches!(hover_background.transition_state, TransitionState::Stopped(false)));
            assert!(!should_request_anim_frame);

            hover_background.transition_state = TransitionState::Stopped(true);

            should_request_anim_frame = hover_background.advance_animation(&ADVANCEMENT_AMOUNT_NANOSECONDS);

            assert!(matches!(hover_background.transition_state, TransitionState::Stopped(true)));
            assert!(!should_request_anim_frame);
        }
    }
}
