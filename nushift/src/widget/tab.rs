// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use druid::{
    text::ArcStr,
    widget::{Painter, Flex, MainAxisAlignment, Label},
    Color, RenderContext, WidgetExt, MouseButton, Widget,
};
use nushift_core::HypervisorEvent;

use crate::model::{RootAndTabData, client_framebuffer::ClientFramebuffer};
use crate::controller::ClickInverse;
use crate::controller::HypervisorCommandHandler;
use super::{button, value};

const TAB_BACKGROUND_COLOR: Color = Color::rgb8(0xa1, 0xf0, 0xf0);
const TAB_HOVER_BACKGROUND_COLOR: Color = Color::rgb8(0xbd, 0xf5, 0xf5);
const TAB_SELECTED_BACKGROUND_COLOR: Color = Color::rgb8(0xe9, 0xfc, 0xfc);

pub fn tab() -> impl Widget<RootAndTabData> {
    let selected_or_non_selected_background = Painter::new(|ctx, data: &RootAndTabData, _| {
        let bounds = ctx.size().to_rect();

        match data.root_data().currently_selected_tab_id {
            Some(ref id) if *id == data.tab_data().id => {
                ctx.fill(bounds, &TAB_SELECTED_BACKGROUND_COLOR);
            }
            _ => {
                if ctx.is_hot() {
                    ctx.fill(bounds, &TAB_HOVER_BACKGROUND_COLOR);
                } else {
                    ctx.fill(bounds, &TAB_BACKGROUND_COLOR);
                }
            }
        }
    });

    let tab = Flex::row()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_child(Label::new(|data: &RootAndTabData, _env: &_| ArcStr::clone(&data.tab_data().title)).with_text_size(value::TAB_TEXT_SIZE))
        .with_child(button::close_button())
        .padding((value::TAB_HORIZONTAL_PADDING, 0.0))
        .background(selected_or_non_selected_background)
        .controller(ClickInverse::new(|_, mouse_event, root_and_tab_data: &mut RootAndTabData, _| {
            match mouse_event.button {
                MouseButton::Left => {
                    let tab_data = root_and_tab_data.tab_data_cloned();
                    root_and_tab_data.root_data_mut().select_tab(&tab_data.id);
                }
                MouseButton::Middle => {
                    let tab_data = root_and_tab_data.tab_data_cloned();
                    root_and_tab_data.root_data_mut().request_close_tab_from_tab_iter(&tab_data.id);
                }
                _ => {}
            }
        }))
        .controller(HypervisorCommandHandler::new(|hypervisor_event, root_and_tab_data: &mut RootAndTabData| {
            let tab_data = root_and_tab_data.tab_data_mut();

            // If tab ID matches, then take.
            if matches!(hypervisor_event.inspect(), Some(Some(tab_id)) if tab_id == tab_data.id) {
                match hypervisor_event.take() {
                    Some(HypervisorEvent::TitleChange(_, new_title)) => {
                        tab_data.title = new_title.as_str().into();
                    }
                    Some(HypervisorEvent::GfxCpuPresent(_, present_buffer_format, size_px, framebuffer)) => {
                        tab_data.client_framebuffer = Some(ClientFramebuffer { present_buffer_format, size_px: size_px.into(), framebuffer });
                    }
                    _ => {}
                }
            }
        }));

    tab
}
