// pathfinder/partitioner/src/cubic_to_quadratic.rs
//
// Copyright © 2018 The Pathfinder Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A version of Lyon's `cubic_to_quadratic` that is less sensitive to floating point error.

use euclid::Point2D;
use lyon_geom::{Arc, CubicBezierSegment, QuadraticBezierSegment};
use lyon_path::PathEvent;

const MAX_APPROXIMATION_ITERATIONS: u8 = 32;

/// Approximates a single cubic Bézier curve with a series of quadratic Bézier curves.
pub struct CubicToQuadraticSegmentIter {
    cubic_curves: Vec<CubicBezierSegment<f32>>,
    error_bound: f32,
    iteration: u8,
}

impl CubicToQuadraticSegmentIter {
    pub fn new(cubic: &CubicBezierSegment<f32>, error_bound: f32) -> CubicToQuadraticSegmentIter {
        let (curve_a, curve_b) = cubic.split(0.5);
        CubicToQuadraticSegmentIter {
            cubic_curves: vec![curve_b, curve_a],
            error_bound: error_bound,
            iteration: 0,
        }
    }
}

impl Iterator for CubicToQuadraticSegmentIter {
    type Item = QuadraticBezierSegment<f32>;

    fn next(&mut self) -> Option<QuadraticBezierSegment<f32>> {
        let mut cubic = match self.cubic_curves.pop() {
            Some(cubic) => cubic,
            None => return None,
        };

        while self.iteration < MAX_APPROXIMATION_ITERATIONS {
            self.iteration += 1;

            // See Sederberg § 2.6, "Distance Between Two Bézier Curves".
            let delta_ctrl_0 = (cubic.from - cubic.ctrl1 * 3.0) + (cubic.ctrl2 * 3.0 - cubic.to);
            let delta_ctrl_1 = (cubic.ctrl1 * 3.0 - cubic.from) + (cubic.to - cubic.ctrl2 * 3.0);
            let max_error = f32::max(delta_ctrl_1.length(), delta_ctrl_0.length()) / 6.0;
            if max_error < self.error_bound {
                break
            }

            let (cubic_a, cubic_b) = cubic.split(0.5);
            self.cubic_curves.push(cubic_b);
            cubic = cubic_a
        }

        let approx_ctrl_0 = (cubic.ctrl1 * 3.0 - cubic.from) * 0.5;
        let approx_ctrl_1 = (cubic.ctrl2 * 3.0 - cubic.to) * 0.5;

        Some(QuadraticBezierSegment {
            from: cubic.from,
            ctrl: approx_ctrl_0.lerp(approx_ctrl_1, 0.5).to_point(),
            to: cubic.to,
        })
    }
}

pub struct ArcToQuadraticSegmentIter {
    segments: Vec<QuadraticBezierSegment<f32>>,
    // error_bound: f32,
}

impl ArcToQuadraticSegmentIter {
    pub fn new(arc: &Arc<f32>) -> ArcToQuadraticSegmentIter {
        let mut segments = vec![];
        arc.for_each_quadratic_bezier(&mut |segment: &QuadraticBezierSegment<f32>| {
            segments.push(*segment);
        });
        ArcToQuadraticSegmentIter {
            segments: segments,
        }
    }
}

impl Iterator for ArcToQuadraticSegmentIter {
    type Item = QuadraticBezierSegment<f32>;

    fn next(&mut self) -> Option<QuadraticBezierSegment<f32>> {
        self.segments.pop()
    }
}

pub struct CubicToQuadraticTransformer<I> where
    I: Iterator<Item = PathEvent>,
{
    inner: I,
    segment_iter: Option<Box<dyn Iterator<Item = QuadraticBezierSegment<f32>>>>,
    last_point: Point2D<f32>,
    error_bound: f32,
}

impl<I> CubicToQuadraticTransformer<I> where I: Iterator<Item = PathEvent> {
    #[inline]
    pub fn new(inner: I, error_bound: f32) -> CubicToQuadraticTransformer<I> {
        CubicToQuadraticTransformer {
            inner: inner,
            segment_iter: None,
            last_point: Point2D::zero(),
            error_bound: error_bound,
        }
    }
}

impl<I> Iterator for CubicToQuadraticTransformer<I> where I: Iterator<Item = PathEvent> {
    type Item = PathEvent;

    fn next(&mut self) -> Option<PathEvent> {
        if let Some(ref mut segment_iter) = self.segment_iter {
            if let Some(quadratic) = segment_iter.next() {
                return Some(PathEvent::QuadraticTo(quadratic.ctrl, quadratic.to))
            }
        }

        self.segment_iter = None;

        match self.inner.next() {
            None => None,
            Some(PathEvent::CubicTo(ctrl1, ctrl2, to)) => {
                let cubic = CubicBezierSegment {
                    from: self.last_point,
                    ctrl1: ctrl1,
                    ctrl2: ctrl2,
                    to: to,
                };
                self.last_point = to;
                self.segment_iter = Some(Box::new(CubicToQuadraticSegmentIter::new(
                    &cubic,
                    self.error_bound
                )));
                self.next()
            }
            Some(PathEvent::MoveTo(to)) => {
                self.last_point = to;
                Some(PathEvent::MoveTo(to))
            }
            Some(PathEvent::LineTo(to)) => {
                self.last_point = to;
                Some(PathEvent::LineTo(to))
            }
            Some(PathEvent::QuadraticTo(ctrl, to)) => {
                self.last_point = to;
                Some(PathEvent::QuadraticTo(ctrl, to))
            }
            Some(PathEvent::Close) => Some(PathEvent::Close),
            Some(PathEvent::Arc(to, vector, angle_from, angle_to)) => {
                let start_angle = (to - self.last_point).angle_from_x_axis() - angle_from;
                let arc = Arc {
                    center: to,
                    radii: vector,
                    start_angle,
                    sweep_angle: angle_to,
                    x_rotation: angle_from,
                };
                self.last_point = to;
                self.segment_iter = Some(Box::new(ArcToQuadraticSegmentIter::new(
                    &arc
                )));
                self.next()
            }
        }
    }
}
