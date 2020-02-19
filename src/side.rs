use crate::voxel::{Data, Voxel};
use nalgebra_glm::{Mat3, Vec3};

pub trait Side {
    const SIDE: usize;
    const DX: isize;
    const DY: isize;
    const DZ: isize;

    fn offset<T: Data>() -> isize;

    fn accept<T: Data>(x: usize, y: usize, z: usize) -> bool;

    fn orientation() -> Mat3;
}

pub struct Left;

impl Side for Left {
    const SIDE: usize = 1;
    const DX: isize = 1;
    const DY: isize = 0;
    const DZ: isize = 0;

    fn offset<T: Data>() -> isize { 
        Voxel::<T>::DX as isize 
    }

    fn accept<T: Data>(x: usize, _: usize, _: usize) -> bool {
        x < Voxel::<T>::LAST
    }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
        ])
    }
}

pub struct Right;

impl Side for Right {
    const SIDE: usize = 0;
    const DX: isize = -1;
    const DY: isize = 0;
    const DZ: isize = 0;

    fn offset<T: Data>() -> isize {
        -(Voxel::<T>::DX as isize)
    }

    fn accept<T: Data>(x: usize, _: usize, _: usize) -> bool {
        x > 0
    }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
        ])
    }
}

pub struct Below;

impl Side for Below {
    const SIDE: usize = 3;
    const DX: isize = 0;
    const DY: isize = 1;
    const DZ: isize = 0;

    fn offset<T: Data>() -> isize {
        Voxel::<T>::DY as isize
    }

    fn accept<T: Data>(_: usize, y: usize, _: usize) -> bool {
        y < Voxel::<T>::LAST
    }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 1.0, 0.0),
        ])
    }
}

pub struct Above;

impl Side for Above {
    const SIDE: usize = 2;
    const DX: isize = 0;
    const DY: isize = -1;
    const DZ: isize = 0;

    fn offset<T: Data>() -> isize {
        -(Voxel::<T>::DY as isize)
    }

    fn accept<T: Data>(_: usize, y: usize, _: usize) -> bool {
        y > 0
    }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, -1.0, 0.0),
        ])
    }
}

pub struct Back;

impl Side for Back {
    const SIDE: usize = 5;
    const DX: isize = 0;
    const DY: isize = 0;
    const DZ: isize = 1;

    fn offset<T: Data>() -> isize {
        Voxel::<T>::DZ as isize
    }

    fn accept<T: Data>(_: usize, _: usize, z: usize) -> bool {
        z < Voxel::<T>::LAST
    }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ])
    }
}

pub struct Front;

impl Side for Front {
    const SIDE: usize = 4;
    const DX: isize = 0;
    const DY: isize = 0;
    const DZ: isize = -1;

    fn offset<T: Data>() -> isize {
        -(Voxel::<T>::DZ as isize)
    }

    fn accept<T: Data>(_: usize, _: usize, z: usize) -> bool {
        z > 0
    }

    fn orientation() -> Mat3 {
        Mat3::from_columns(&[
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, -1.0),
        ])
    }
}
