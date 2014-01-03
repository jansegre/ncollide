use nalgebra::na::{AbsoluteRotate, Translation};
use bounding_volume::{HasAABB, AABB};
use geom::Box;
use math::M;

impl HasAABB for Box {
    #[inline]
    fn aabb(&self, m: &M) -> AABB {
        let center          = m.translation();
        let ws_half_extents = m.absolute_rotate(&(self.half_extents() + self.margin()));

        AABB::new(center - ws_half_extents, center + ws_half_extents)
    }
}