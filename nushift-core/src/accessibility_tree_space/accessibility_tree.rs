// Copyright 2023 The Nushift Authors.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct AccessibilityTree {
    surfaces: Vec<Surface>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
struct Surface {
    display_list: Vec<DisplayItem>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
enum DisplayItem {
    Text {
        aabb: (Vec<VirtualPoint>, Vec<VirtualPoint>),
        text: String,
    },
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(transparent)]
struct VirtualPoint(f64);
