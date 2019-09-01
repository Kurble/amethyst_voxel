#[derive(Debug, Clone, Copy)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Pos {
    #[inline]
    pub fn new(x: f32, y: f32, z: f32) -> Pos {
        Self { x, y, z }
    }
    #[inline]
    pub fn left(&self, scale: f32) -> Pos {
        Self { x: self.x - scale, y: self.y, z: self.z, }
    }
    #[inline]
    pub fn right(&self, scale: f32) -> Pos {
        Self { x: self.x + scale, y: self.y, z: self.z, }
    }
    #[inline]
    pub fn below(&self, scale: f32) -> Pos {
        Self { x: self.x, y: self.y - scale, z: self.z, }
    }
    #[inline]
    pub fn above(&self, scale: f32) -> Pos {
        Self { x: self.x, y: self.y + scale, z: self.z, }
    }
    #[inline]
    pub fn back(&self, scale: f32) -> Pos {
        Self { x: self.x, y: self.y, z: self.z - scale, }
    }
    #[inline]
    pub fn front(&self, scale: f32) -> Pos {
        Self { x: self.x, y: self.y, z: self.z + scale, }
    }
}