use druid::{WidgetPod, Widget};
use druid::widget::{ListIter};

use crate::widget_data::TabData;

pub struct TabList {
    children: Vec<WidgetPod<TabData, Box<dyn Widget<TabData>>>>,
}

// TODO
// impl<T: ListIter<TabData>> Widget<T> for TabList {

// }
