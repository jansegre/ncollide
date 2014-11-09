//! Non-persistant geometric queries.

#[doc(inline)]
pub use self::contacts_internal::Contact;
#[doc(inline)]
pub use self::contacts_internal::contacts_with::{contact, contacts};
// #[doc(inline)]
// pub use self::distance_internal::distance_with::distance;

pub mod algorithms;
// pub mod closest_points;
// pub mod intersection_test;
// pub mod time_of_impact;
pub mod contacts_internal;
// pub mod distance_internal;