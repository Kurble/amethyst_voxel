use crate::voxel::{Data, Const};
use nalgebra_glm::{Vec3, Mat3};

pub trait Side<T: Data> {
    const OFFSET: isize;
    const SIDE: usize;

    const DX: isize;
    const DY: isize;
    const DZ: isize;

    fn accept(x: usize, y: usize, z: usize) -> bool;

    fn orientation() -> Mat3;
}

pub struct Left;

impl<T: Data> Side<T> for Left {
    const OFFSET: isize = (Const::<T>::DX as isize);

    const SIDE: usize = 1;

    const DX: isize = 1;
    const DY: isize = 0;
    const DZ: isize = 0;

    fn accept(x: usize, _: usize, _: usize) -> bool { x < Const::<T>::LAST }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new( 0.0,  0.0,  1.0),
            Vec3::new( 0.0,  1.0,  0.0),
            Vec3::new( 1.0,  0.0,  0.0),
        ])
    }
}

pub struct Right;

impl<T: Data> Side<T> for Right {
    const OFFSET: isize = -(Const::<T>::DX as isize);

    const SIDE: usize = 0;

    const DX: isize = -1;
    const DY: isize = 0;
    const DZ: isize = 0;

    fn accept(x: usize, _: usize, _: usize) -> bool { x > 0 }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new( 0.0,  0.0, -1.0),
            Vec3::new( 0.0,  1.0,  0.0),
            Vec3::new(-1.0,  0.0,  0.0),
        ])
    }
}

pub struct Below;

impl<T: Data> Side<T> for Below {
    const OFFSET: isize = Const::<T>::DY as isize;

    const SIDE: usize = 3;

    const DX: isize = 0;
    const DY: isize = 1;
    const DZ: isize = 0;

    fn accept(_: usize, y: usize, _: usize) -> bool { y < Const::<T>::LAST }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(-1.0,  0.0,  0.0),
            Vec3::new( 0.0,  0.0, -1.0),
            Vec3::new( 0.0,  1.0,  0.0),
        ])
    }
}

pub struct Above;

impl<T: Data> Side<T> for Above {
    const OFFSET: isize = -(Const::<T>::DY as isize);

    const SIDE: usize = 2;

    const DX: isize = 0;
    const DY: isize = -1;
    const DZ: isize = 0;

    fn accept(_: usize, y: usize, _: usize) -> bool { y > 0 }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(-1.0,  0.0,  0.0),
            Vec3::new( 0.0,  0.0,  1.0),
            Vec3::new( 0.0, -1.0,  0.0),
        ])
    }
}

pub struct Back;

impl<T: Data> Side<T> for Back {
    const OFFSET: isize = Const::<T>::DZ as isize;

    const SIDE: usize = 5;

    const DX: isize = 0;
    const DY: isize = 0;
    const DZ: isize = 1;

    fn accept(_: usize, _: usize, z: usize) -> bool { z < Const::<T>::LAST }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(-1.0,  0.0,  0.0),
            Vec3::new( 0.0,  1.0,  0.0),
            Vec3::new( 0.0,  0.0,  1.0),
        ])
    }
}

pub struct Front;

impl<T: Data> Side<T> for Front {
    const OFFSET: isize = -(Const::<T>::DZ as isize);

    const SIDE: usize = 4;

    const DX: isize = 0;
    const DY: isize = 0;
    const DZ: isize = -1;

    fn accept(_: usize, _: usize, z: usize) -> bool { z > 0 }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new( 1.0,  0.0,  0.0),
            Vec3::new( 0.0,  1.0,  0.0),
            Vec3::new( 0.0,  0.0, -1.0),
        ])
    }
}
