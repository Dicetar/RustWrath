//! Unit-to-unit collision for our own mover, a Souls-mode feature ([`crate::souls`]).
//!
//! The stock client and the server have none: a body walks through every unit. Elden Ring's
//! characters are solid capsules, so a swing's step forward stops at the enemy's body and running
//! into one slides round it. The drive gathers the hostile bodies near us each frame
//! ([`super::Player::unit_bodies`]: position and `UNIT_FIELD_BOUNDINGRADIUS`) and the controller
//! slides the mover's frame round them ([`block`]). Only our predicted pose moves, as ordinary
//! movement on the wire; units the server moves into us are not pushed, and we are never moved
//! further out than where the frame started, so the slide cannot shove the body into a wall.

use bevy::prelude::*;

/// Our body's radius, yards: a human's `UNIT_FIELD_BOUNDINGRADIUS` (0.306-0.389) rounded up.
pub(crate) const SELF_BODY_YARDS: f32 = 0.4;
/// A unit's body radius is its `UNIT_FIELD_BOUNDINGRADIUS` held inside this range, yards: absent
/// reads 0, and a dragon's would hold a melee swing out of its own reach.
pub(crate) const BODY_YARDS: (f32, f32) = (0.3, 2.0);
/// A body above or below the feet by more than this many yards does not collide.
pub const BODY_HEIGHT_YARDS: f32 = 3.0;
/// Bodies further than this, yards, are not gathered.
pub const GATHER_YARDS: f32 = 15.0;

/// The contact distance between our centre and a body of bounding radius `radius`, yards.
pub fn contact(radius: f32) -> f32 {
    SELF_BODY_YARDS + radius.clamp(BODY_YARDS.0, BODY_YARDS.1)
}

/// The frame's move from `from` to `to`, slid round `bodies` (position, bounding radius): inside a
/// body's contact circle, the end is put back out to the contact distance, or to where the frame
/// started from it if that was nearer (already overlapping: the body may not go deeper, but is not
/// pushed out either). Tangential movement is kept, so the body slides round. Height is untouched.
pub(crate) fn block(from: Vec3, to: Vec3, bodies: &[(Vec3, f32)]) -> Vec3 {
    let mut end = to;
    for &(at, radius) in bodies {
        if (end.y - at.y).abs() > BODY_HEIGHT_YARDS {
            continue;
        }
        let reach = contact(radius);
        let center = at.xz();
        let off = end.xz() - center;
        let distance = off.length();
        if distance >= reach {
            continue;
        }
        // Where the frame started from this body: the floor the slide may push back out to.
        let started = (from.xz() - center).length();
        let floor = reach.min(started);
        if distance >= floor {
            continue;
        }
        // Straight onto the centre: push back the way the frame came.
        let normal = if distance > 1e-4 {
            off / distance
        } else {
            let back = from.xz() - end.xz();
            if back.length() > 1e-4 {
                back.normalize()
            } else {
                continue;
            }
        };
        let out = center + normal * floor;
        end = Vec3::new(out.x, end.y, out.y);
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run straight into a body stops at contact; a glancing one slides round it; one already
    /// overlapping goes no deeper but is not shoved out; a body on another floor is passed.
    #[test]
    fn the_mover_slides_round_a_body() {
        let body = [(Vec3::new(0.0, 0.0, 0.0), 1.0)];
        let reach = contact(1.0);
        // Head-on along +x, ending inside: put back at contact.
        let end = block(Vec3::new(-3.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0), &body);
        assert!((end.x + reach).abs() < 1e-5 && end.z.abs() < 1e-5, "{end}");
        // Dead centre: back along the way it came.
        let end = block(Vec3::new(-3.0, 0.0, 0.0), Vec3::ZERO, &body);
        assert!((end.x + reach).abs() < 1e-5, "{end}");
        // Glancing: still at contact, but along the side it hit.
        let end = block(Vec3::new(-3.0, 0.0, 0.5), Vec3::new(-1.0, 0.0, 0.5), &body);
        assert!(
            (end.xz().length() - reach).abs() < 1e-5 && end.z > 0.0,
            "{end}"
        );
        // Already overlapping at 1.0 from the centre, stepping in to 0.8: held at 1.0.
        let end = block(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(-0.8, 0.0, 0.0), &body);
        assert!((end.x + 1.0).abs() < 1e-5, "{end}");
        // Stepping out of an overlap is free.
        let to = Vec3::new(-1.2, 0.0, 0.0);
        assert_eq!(block(Vec3::new(-1.0, 0.0, 0.0), to, &body), to);
        // Another floor, or out of reach: untouched.
        let to = Vec3::new(-1.0, 5.0, 0.0);
        assert_eq!(block(Vec3::new(-3.0, 5.0, 0.0), to, &body), to);
        let to = Vec3::new(-2.0, 0.0, 0.0);
        assert_eq!(block(Vec3::new(-3.0, 0.0, 0.0), to, &body), to);
    }
}
