//! Shape of tensor.
use core::fmt::Debug;
/// Shape of observation or action.
pub trait Shape: Clone + Debug {
    /// Returns the shape of Shape of an array.
    fn shape() -> &'static [usize];

    /// Returns `true` if you would like to squeeze the first dimension of the array
    /// before conversion into an numpy array in Python. The first dimension may
    /// correspond to process indices for vectorized environments.
    fn squeeze_first_dim() -> bool {
        false
    }
}

/// Defines a struct that implements [Shape].
///
/// # Example
///
/// ```
/// use border_core::shape;
///
/// shape!(ObsShape, [4, 2]);
///
/// println!("{:?}", ObsShape::shape());
/// ```
#[macro_export]
macro_rules! shape {
    ($struct_:ident, [$($elem_:expr),+]) => {
        #[derive(Clone, Debug)]
        struct $struct_ {}
        impl border_core::Shape for $struct_ {
            fn shape() -> &'static [usize] {
                &[$($elem_),+]
            }
        }
    };
    ($struct_:ident, [$($elem_:expr),+], squeeze_first_dim) => {
        #[derive(Clone, Debug)]
        struct $struct_ {}
        impl border_core::Shape for $struct_ {
            fn shape() -> &'static [usize] {
                &[$($elem_),+]
            }
            fn squeeze_first_dim() -> bool {
                true
            }
        }
    };
}
