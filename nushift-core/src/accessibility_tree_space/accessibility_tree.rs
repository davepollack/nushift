use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct AccessibilityTree {
    surfaces: Vec<Surface>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Surface {
    display_list: Vec<DisplayItem>,
}

#[derive(Debug, Deserialize, Serialize)]
enum DisplayItem {
    Text(Text),
}

#[derive(Debug, Deserialize, Serialize)]
struct Text {
    aabb: (Vec<f64>, Vec<f64>),
    text: String,
}
