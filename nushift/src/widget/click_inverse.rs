use druid::{Env, EventCtx, Data, Widget, widget::Controller, Event, LifeCycleCtx, LifeCycle, MouseEvent};

/// Like `druid::widget::Click`, but call `child.event` first to let the child
/// (e.g. a close button) handle the event first, because we (e.g. the tab
/// containing the close button) don't want to trigger the "select" action if the
/// close button was clicked.
pub struct ClickInverse<T> {
    action: Box<dyn Fn(&mut EventCtx, &MouseEvent, &mut T, &Env)>,
}

impl<T: Data> ClickInverse<T> {
    pub fn new(action: impl Fn(&mut EventCtx, &MouseEvent, &mut T, &Env) + 'static) -> Self {
        ClickInverse {
            action: Box::new(action),
        }
    }
}

impl<T: Data, W: Widget<T>> Controller<T, W> for ClickInverse<T> {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        child.event(ctx, event, data, env);

        // Only continue event processing if child did not handle it.
        if !ctx.is_handled() {
            match event {
                Event::MouseDown(_) => {
                    ctx.set_active(true);
                    ctx.request_paint();
                    ctx.set_handled();
                }
                Event::MouseUp(mouse_event) => {
                    if ctx.is_active() {
                        ctx.set_active(false);
                        if ctx.is_hot() {
                            (self.action)(ctx, mouse_event, data, env);
                        }
                        ctx.request_paint();
                        ctx.set_handled();
                    }
                }
                _ => {}
            }
        }
    }

    fn lifecycle(&mut self, child: &mut W, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &T, env: &Env) {
        if let LifeCycle::HotChanged(_) | LifeCycle::FocusChanged(_) = event {
            ctx.request_paint();
        }

        child.lifecycle(ctx, event, data, env);
    }
}
