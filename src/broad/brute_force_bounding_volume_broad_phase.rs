use std::ptr;
use std::util;
use util::hash_map::HashMap;
use util::hash::UintTWHash;
use util::pair::{Pair, PairTWHash};
use broad::dispatcher::Dispatcher;
use bounding_volume::bounding_volume::{HasBoundingVolume, LooseBoundingVolume};

/// Associatio of an object with its loose bounding volume.
#[deriving(ToStr, Eq, Clone)]
pub struct BoundingVolumeProxy<N, B, BV> {
    /// The objects loose bounding volume.
    bounding_volume: BV,
    /// The object.
    body:            @mut B
}

impl<N: Clone, BV: LooseBoundingVolume<N>, B: HasBoundingVolume<BV>> BoundingVolumeProxy<N, B, BV> {
    /// Builds a new brute force broad phase based on loose bounding volumes.
    ///
    /// # Arguments:
    ///     * `b` - collision dispatcher.
    ///     * `margin` - loosening margin.
    pub fn new(b: @mut B, margin: N) -> BoundingVolumeProxy<N, B, BV> {
        BoundingVolumeProxy {
            bounding_volume: b.bounding_volume().loosened(margin),
            body:            b
        }
    }

    /// Updates this proxy. Returns `true` if the stored bounding volume has been changed.
    pub fn update(&mut self, margin: &N) -> bool {
        let mut new_bv = self.body.bounding_volume();

        if !self.bounding_volume.contains(&new_bv) {
            new_bv.loosen(margin.clone());
            self.bounding_volume = new_bv;

            true
        }
        else {
            false
        }
    }
}

/// Broad phase with quadratic complixity but sped up using looze bounding volumes. Interference
/// detection is executed only for objects which have their bounding volumes updated. The number of
/// such objects is assumed to be small enough to not exhibit the quadratic nature of the detection
/// algorithm.
///
/// Dont use this broad phase. It exists mainly as a transition broad phase between the Brute Force
/// one and the DBVH/SAP based broad phases.
pub struct BruteForceBoundingVolumeBroadPhase<N, B, BV, D, DV> {
    priv objects:    ~[@mut BoundingVolumeProxy<N, B, BV>], // active   objects
    priv sobjects:   ~[@mut BoundingVolumeProxy<N, B, BV>], // inactive objects
    priv rb2bv:      HashMap<uint, uint, UintTWHash>,
    priv pairs:      HashMap<Pair<BoundingVolumeProxy<N, B, BV>>, DV, PairTWHash>, // pair manager
    priv dispatcher: D,
    priv margin:     N,
    priv to_update:  ~[@mut BoundingVolumeProxy<N, B, BV>],
    priv update_off: uint // incremental pairs removal index
}

impl<N:  Clone,
     B:  'static + HasBoundingVolume<BV>,
     BV: 'static + LooseBoundingVolume<N>,
     D:  Dispatcher<B, DV>,
     DV>
BruteForceBoundingVolumeBroadPhase<N, B, BV, D, DV> {
    /// Creates a new bounding volume based brute force broad phase.
    pub fn new(dispatcher: D, margin: N) -> BruteForceBoundingVolumeBroadPhase<N, B, BV, D, DV> {
        BruteForceBoundingVolumeBroadPhase {
            objects:    ~[],
            sobjects:   ~[],
            to_update:  ~[],
            rb2bv:      HashMap::new(UintTWHash),
            pairs:      HashMap::new(PairTWHash),
            dispatcher: dispatcher,
            update_off: 0,
            margin:     margin
        }
    }

    /// The pair manager of this broad phase.
    pub fn pairs<'r>(&'r self) -> &'r HashMap<Pair<BoundingVolumeProxy<N, B, BV>>, DV, PairTWHash> {
        &'r self.pairs
    }

    /// The pair manager of this broad phase.
    pub fn pairs_mut<'r>(&'r mut self)
                         -> &'r mut HashMap<Pair<BoundingVolumeProxy<N, B, BV>>, DV, PairTWHash> {
        &'r mut self.pairs
    }

    /// Adds an element to this broad phase.
    pub fn add(&mut self, rb: @mut B) {
        let proxy = @mut BoundingVolumeProxy::new(rb, self.margin.clone());
        self.objects.push(proxy);
        self.to_update.push(proxy);
    }

    /// Removes an element from this broad phase.
    pub fn remove(&mut self, _: @mut B) {
        fail!("Not yet implemented.");
    }

    /// Marks and object as active or inactive. The bounding volume of an inactive object is never
    /// updated. Activating/deactivating an already active/inactive objecs leads to undefined
    /// behaviour.
    pub fn set_active(&mut self, b: @mut B, active: bool) {
        let (key, at) =
            match self.rb2bv.find_mut(&(ptr::to_mut_unsafe_ptr(b) as uint)) {
                None    => fail!("Unable to change the active state of an unknown object."),
                Some(i) => {
                    if active {
                        // remove from sobjects…
                        let proxy  = self.sobjects[*i];
                        let lproxy = self.sobjects.pop();
                        self.sobjects[*i] = lproxy;

                        // … then add to objects
                        self.objects.push(proxy);

                        let mut at = self.objects.len() - 1;

                        util::swap(&mut at, i);

                        (lproxy, at)
                    }
                    else {
                        // remove from objects…
                        let proxy  = self.objects[*i];
                        let lproxy = self.objects.pop();
                        self.objects[*i] = lproxy;

                        // … then add to sobjects
                        self.sobjects.push(proxy);

                        let mut at = self.sobjects.len() - 1;

                        util::swap(&mut at, i);

                        (lproxy, at)
                    }
                }
            };

        self.rb2bv.insert(ptr::to_mut_unsafe_ptr(key) as uint, at);
    }

    /// Updates the collision pairs based on the objects bounding volumes.
    pub fn update(&mut self) {
        let mut new_colls = 0u;

        for &b in self.objects.mut_iter() {
            if b.update(&self.margin) {
                self.to_update.push(b)
            }
        }

        for &b1 in self.to_update.iter() {
            for &b2 in self.objects.iter() {
                if self.dispatcher.is_valid(b1.body, b2.body) {
                    if b2.bounding_volume.intersects(&b1.bounding_volume) {
                        self.pairs.find_or_insert_lazy(
                            Pair::new(b1, b2),
                            || self.dispatcher.dispatch(b1.body, b2.body)
                        );

                        new_colls = new_colls + 1;
                    }
                }
            }
        }

        if new_colls != 0 {
            let len          = self.pairs.len();
            let num_removals = new_colls.clamp(&(len / 10), &len);

            for i in range(self.update_off, self.update_off + num_removals) {
                let id = i % self.pairs.len();

                let remove = {
                    let elts  = self.pairs.elements();
                    let entry = &elts[id];

                    if (!entry.key.first.bounding_volume.intersects(&entry.key.second.bounding_volume)) {
                        true
                    }
                    else {
                        false
                    }
                };

                if remove {
                    self.pairs.remove_elem_at(id);
                }
            }

            self.update_off = (self.update_off + num_removals) % self.pairs.len();
        }

        self.to_update.clear()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use nalgebra::vec::Vec3;
    use nalgebra::traits::translation::{Translatable, Translation};
    use geom::ball::Ball;
    use bounding_volume::aabb::WithAABB;
    use broad::dispatcher::NoIdDispatcher;

    #[test]
    fn test_bfbv_empty() {
        let dispatcher: NoIdDispatcher<WithAABB<Ball<float, Vec3<float>>>> = NoIdDispatcher;
        let mut bf     = BruteForceBoundingVolumeBroadPhase::new(dispatcher, 0.2);
        let ball       = Ball::new(Vec3::new(0.0, 0.0, 0.0), 0.3);

        for i in range(-10, 10) {
            for j in range(-10, 10) {
                let tball = ball.translated(&Vec3::new(i as float * 30.0, j as float * 30.0, 0.0));

                bf.add(@mut WithAABB(tball));
            }
        }

        bf.update();

        assert_eq!(bf.pairs().len(), 0)
    }

    #[test]
    fn test_bfbv_nbh_collide() {
        let dispatcher: NoIdDispatcher<WithAABB<Ball<float, Vec3<float>>>> = NoIdDispatcher;
        let mut bf     = BruteForceBoundingVolumeBroadPhase::new(dispatcher, 0.2);
        let ball       = Ball::new(Vec3::new(0.0, 0.0, 0.0), 0.3);

        // create a grid
        for i in range(-10, 10) {
            for j in range(-10, 10) {
                let tball = ball.translated(&Vec3::new(i as float * 0.9, j as float * 0.9, 0.0));

                bf.add(@mut WithAABB(tball));
            }
        }

        bf.update();

        assert_eq!(
            bf.pairs().len(),
            (18 * 18 * 8 + // internal rectangles have 8 neighbors
             18 * 4 * 5  + // border (excluding corners) rectangles have 5 neighbors
             4 * 3)        // corners have 3 neighbors
            / 2            // remove all duplicates
        )
    }

    #[test]
    fn test_dbvt_nbh_move_collide() {
        let dispatcher: NoIdDispatcher<WithAABB<Ball<float, Vec3<float>>>> = NoIdDispatcher;
        let mut bf     = BruteForceBoundingVolumeBroadPhase::new(dispatcher, 0.2);
        let ball       = Ball::new(Vec3::new(0.0, 0.0, 0.0), 0.3);

        let mut to_move = ~[];

        // create a grid
        for i in range(-10, 10) {
            for j in range(-10, 10) {
                let tball = ball.translated(&Vec3::new(i as float * 0.9, j as float * 0.9, 0.0));

                let to_add = @mut WithAABB(tball);
                bf.add(to_add);
                to_move.push(to_add);
            }
        }

        for e in to_move.consume_iter() {
            e.translate_by(&Vec3::new(10.0, 10.0, 10.0))
        }

        bf.update();

        assert_eq!(
            bf.pairs().len(),
            (18 * 18 * 8 + // internal rectangles have 8 neighbors
             18 * 4 * 5  + // border (excluding corners) rectangles have 5 neighbors
             4 * 3)        // corners have 3 neighbors
            / 2            // remove all duplicates
        )
    }

    #[test]
    fn test_bfbv_quadratic_collide() {
        let dispatcher: NoIdDispatcher<WithAABB<Ball<float, Vec3<float>>>> = NoIdDispatcher;
        let mut bf     = BruteForceBoundingVolumeBroadPhase::new(dispatcher, 0.2);
        let ball       = Ball::new(Vec3::new(0.0, 0.0, 0.0), 0.3);

        do 400.times {
            bf.add(@mut WithAABB(ball))
        }

        bf.update();

        assert_eq!(bf.pairs().len(), (399 * (399 + 1)) / 2)
    }
}
