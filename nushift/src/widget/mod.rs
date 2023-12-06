// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

mod button {
    mod hover_background;
    mod new_tab_button;
    mod close_button;

    pub use new_tab_button::new_tab_button;
    pub use close_button::close_button;
}
pub mod client_area;
mod tab;
mod tab_list;
mod top_bar;
mod value;

pub use top_bar::top_bar;
