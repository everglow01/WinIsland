#[derive(Clone, Copy)]
pub(crate) struct Point {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl Point {
    pub(crate) const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Vec2 {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl Vec2 {
    pub(crate) const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Rect {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) bottom: f32,
}

impl Rect {
    pub(crate) const fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        }
    }

    pub(crate) const fn width(&self) -> f32 {
        self.right - self.left
    }

    pub(crate) const fn height(&self) -> f32 {
        self.bottom - self.top
    }

    pub(crate) const fn center_x(&self) -> f32 {
        (self.left + self.right) * 0.5
    }

    pub(crate) const fn center_y(&self) -> f32 {
        (self.top + self.bottom) * 0.5
    }

    pub(crate) fn offset(self, delta: Vec2) -> Self {
        Self {
            left: self.left + delta.x,
            top: self.top + delta.y,
            right: self.right + delta.x,
            bottom: self.bottom + delta.y,
        }
    }

    pub(crate) fn inset(self, delta: f32) -> Self {
        Self {
            left: self.left + delta,
            top: self.top + delta,
            right: self.right - delta,
            bottom: self.bottom - delta,
        }
    }
}
