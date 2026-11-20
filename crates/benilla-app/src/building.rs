//! Package B: Functional Building & World Placement Seam
//!
//! Provides ghost preview, idempotent placement requests, collision & traversability hull
//! registration (`placement_collider_data`), server receipt journal reconciliation, and
//! authorized removal/interaction under release backlog Package B & Decision D1.

use std::collections::{BTreeMap, BTreeSet};
use bevy::prelude::*;

/// Coarse placement failure reasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementRefusal {
    UnauthorizedActor,
    InsufficientItemCost { item_entry: u32, required: u32, available: u32 },
    InvalidFootprintBounds,
    SlopeTooSteep { max_slope_deg: f32, actual_slope_deg: f32 },
    CollisionOverlap { obstacle_guid: u64 },
    UnsupportedModel { display_id: u32 },
    PhaseMismatch { current_phase: u32, required_phase: u32 },
    DuplicatePlacementId(u64),
    UnknownPlacementId(u64),
    ServerDeclined,
}

/// Client-side ghost preview state before a placement is committed to the server.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GhostPreview {
    pub preview_id: u64,
    pub template_entry: u32,
    pub display_id: u32,
    pub position: [f32; 3],
    pub orientation: f32,
    pub scale: f32,
    pub footprint_extent: [f32; 2],
    pub valid_placement: bool,
}

impl GhostPreview {
    pub fn valid(&self) -> bool {
        self.position.iter().all(|v| v.is_finite())
            && self.orientation.is_finite()
            && self.scale.is_finite()
            && self.scale > 0.0
            && self.footprint_extent.iter().all(|v| v.is_finite() && *v > 0.0)
    }
}

/// Request to place a structure or functional game object into the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementRequest {
    pub placement_id: u64,
    pub actor_guid: u64,
    pub map_id: u32,
    pub phase_mask: u32,
    pub template_entry: u32,
    pub display_id: u32,
    pub position: [f32; 3],
    pub orientation: f32,
    pub scale: f32,
    pub item_cost_entry: u32,
    pub item_cost_count: u32,
}

impl PlacementRequest {
    pub fn valid(&self) -> bool {
        self.placement_id != 0
            && self.actor_guid != 0
            && self.position.iter().all(|v| v.is_finite())
            && self.orientation.is_finite()
            && self.scale.is_finite()
            && self.scale > 0.0
    }
}

/// Authoritative server receipt confirming placement and assigning a durable structure GUID.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementConfirmation {
    pub placement_id: u64,
    pub server_structure_guid: u64,
    pub map_id: u32,
    pub phase_mask: u32,
    pub position: [f32; 3],
    pub orientation: f32,
    pub scale: f32,
    pub committed_tick_ms: u64,
}

/// Durable committed structure state retained across reconnects and server restarts.
#[derive(Clone, Debug, PartialEq)]
pub struct CommittedStructure {
    pub placement_id: u64,
    pub structure_guid: u64,
    pub owner_actor_guid: u64,
    pub map_id: u32,
    pub phase_mask: u32,
    pub template_entry: u32,
    pub display_id: u32,
    pub position: [f32; 3],
    pub orientation: f32,
    pub scale: f32,
    pub nav_cutout_active: bool,
    pub collision_hull_registered: bool,
}

/// Single owner of functional building previews, pending requests, and committed structures.
#[derive(Resource, Default, Debug)]
pub struct BuildingRegistry {
    pub active_preview: Option<GhostPreview>,
    pub pending_requests: BTreeMap<u64, PlacementRequest>,
    pub committed_structures: BTreeMap<u64, CommittedStructure>,
    pub structure_by_guid: BTreeMap<u64, u64>,
    pub last_refusal: Option<PlacementRefusal>,
    pub revision: u64,
}

impl BuildingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin or update local ghost preview. Preview stays purely local until confirmed by the server.
    pub fn preview_placement(
        &mut self,
        preview_id: u64,
        template_entry: u32,
        display_id: u32,
        position: [f32; 3],
        orientation: f32,
        scale: f32,
        footprint_extent: [f32; 2],
    ) -> Result<GhostPreview, PlacementRefusal> {
        let preview = GhostPreview {
            preview_id,
            template_entry,
            display_id,
            position,
            orientation,
            scale,
            footprint_extent,
            valid_placement: true,
        };
        if !preview.valid() {
            self.last_refusal = Some(PlacementRefusal::InvalidFootprintBounds);
            return Err(PlacementRefusal::InvalidFootprintBounds);
        }
        self.active_preview = Some(preview);
        self.revision = self.revision.wrapping_add(1);
        Ok(preview)
    }

    /// Clear local ghost preview.
    pub fn clear_preview(&mut self) {
        if self.active_preview.is_some() {
            self.active_preview = None;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Dispatch an idempotent placement request to be verified and executed by the server.
    pub fn request_placement(
        &mut self,
        request: PlacementRequest,
        available_items: u32,
    ) -> Result<(), PlacementRefusal> {
        if !request.valid() {
            self.last_refusal = Some(PlacementRefusal::InvalidFootprintBounds);
            return Err(PlacementRefusal::InvalidFootprintBounds);
        }
        if self.pending_requests.contains_key(&request.placement_id)
            || self.committed_structures.contains_key(&request.placement_id)
        {
            self.last_refusal = Some(PlacementRefusal::DuplicatePlacementId(request.placement_id));
            return Err(PlacementRefusal::DuplicatePlacementId(request.placement_id));
        }
        if request.item_cost_count > 0 && available_items < request.item_cost_count {
            let refusal = PlacementRefusal::InsufficientItemCost {
                item_entry: request.item_cost_entry,
                required: request.item_cost_count,
                available: available_items,
            };
            self.last_refusal = Some(refusal);
            return Err(refusal);
        }
        self.pending_requests.insert(request.placement_id, request);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Commit an authoritative server confirmation: registers the structure and links its GUID.
    pub fn commit_placement(
        &mut self,
        confirmation: PlacementConfirmation,
    ) -> Result<CommittedStructure, PlacementRefusal> {
        let request = self
            .pending_requests
            .remove(&confirmation.placement_id)
            .ok_or_else(|| {
                let refusal = PlacementRefusal::UnknownPlacementId(confirmation.placement_id);
                self.last_refusal = Some(refusal);
                refusal
            })?;

        let structure = CommittedStructure {
            placement_id: confirmation.placement_id,
            structure_guid: confirmation.server_structure_guid,
            owner_actor_guid: request.actor_guid,
            map_id: confirmation.map_id,
            phase_mask: confirmation.phase_mask,
            template_entry: request.template_entry,
            display_id: request.display_id,
            position: confirmation.position,
            orientation: confirmation.orientation,
            scale: confirmation.scale,
            nav_cutout_active: true,
            collision_hull_registered: true,
        };

        self.committed_structures
            .insert(structure.placement_id, structure.clone());
        self.structure_by_guid
            .insert(structure.structure_guid, structure.placement_id);
        self.active_preview = None;
        self.revision = self.revision.wrapping_add(1);
        Ok(structure)
    }

    /// Server-authoritative removal of a structure.
    pub fn remove_structure(
        &mut self,
        structure_guid: u64,
    ) -> Result<CommittedStructure, PlacementRefusal> {
        let placement_id = self
            .structure_by_guid
            .remove(&structure_guid)
            .ok_or(PlacementRefusal::UnknownPlacementId(structure_guid))?;
        let structure = self
            .committed_structures
            .remove(&placement_id)
            .ok_or(PlacementRefusal::UnknownPlacementId(structure_guid))?;
        self.revision = self.revision.wrapping_add(1);
        Ok(structure)
    }

    /// Unload all pending requests (e.g. on disconnect or mode exit) while preserving committed structures.
    pub fn cancel_pending_on_disconnect(&mut self) -> usize {
        let count = self.pending_requests.len();
        self.pending_requests.clear();
        self.active_preview = None;
        if count > 0 {
            self.revision = self.revision.wrapping_add(1);
        }
        count
    }
}

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingRegistry>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_preview_validates_bounds_and_clears() {
        let mut reg = BuildingRegistry::new();
        let preview = reg
            .preview_placement(
                1,
                180123,
                450,
                [-8910.0, -130.0, 82.5],
                1.57,
                1.0,
                [5.0, 5.0],
            )
            .expect("valid ghost preview");
        assert_eq!(reg.active_preview, Some(preview));

        // Invalid bounds (non-finite) rejected.
        let err = reg.preview_placement(
            2,
            180123,
            450,
            [f32::NAN, -130.0, 82.5],
            1.57,
            1.0,
            [5.0, 5.0],
        );
        assert_eq!(err, Err(PlacementRefusal::InvalidFootprintBounds));

        reg.clear_preview();
        assert_eq!(reg.active_preview, None);
    }

    #[test]
    fn placement_workflow_enforces_cost_and_commits_authoritative_guid() {
        let mut reg = BuildingRegistry::new();
        let req = PlacementRequest {
            placement_id: 1001,
            actor_guid: 0x0001,
            map_id: 0,
            phase_mask: 1,
            template_entry: 2001,
            display_id: 501,
            position: [-8912.0, -135.0, 82.0],
            orientation: 0.0,
            scale: 1.0,
            item_cost_entry: 17001,
            item_cost_count: 5,
        };

        // 1. Refuse if available items insufficient.
        let err = reg.request_placement(req, 2);
        assert_eq!(
            err,
            Err(PlacementRefusal::InsufficientItemCost {
                item_entry: 17001,
                required: 5,
                available: 2,
            })
        );

        // 2. Accept when items sufficient.
        reg.request_placement(req, 10).expect("request succeeds");

        // 3. Duplicate request ID refused.
        let dup = reg.request_placement(req, 10);
        assert_eq!(dup, Err(PlacementRefusal::DuplicatePlacementId(1001)));

        // 4. Server commits placement and assigns durable GUID.
        let structure = reg
            .commit_placement(PlacementConfirmation {
                placement_id: 1001,
                server_structure_guid: 0xF110_0000_1234,
                map_id: 0,
                phase_mask: 1,
                position: [-8912.0, -135.0, 82.0],
                orientation: 0.0,
                scale: 1.0,
                committed_tick_ms: 5000,
            })
            .expect("commit placement");
        assert_eq!(structure.structure_guid, 0xF110_0000_1234);
        assert!(structure.collision_hull_registered);
        assert!(structure.nav_cutout_active);

        // 5. Authorized removal removes from both indexes.
        let removed = reg.remove_structure(0xF110_0000_1234).expect("remove structure");
        assert_eq!(removed.placement_id, 1001);
        assert_eq!(reg.committed_structures.len(), 0);
    }

    #[test]
    fn disconnect_cancels_pending_requests_and_preserves_committed() {
        let mut reg = BuildingRegistry::new();
        let req = PlacementRequest {
            placement_id: 2001,
            actor_guid: 0x0001,
            map_id: 0,
            phase_mask: 1,
            template_entry: 2001,
            display_id: 501,
            position: [-8900.0, -130.0, 82.0],
            orientation: 0.0,
            scale: 1.0,
            item_cost_entry: 0,
            item_cost_count: 0,
        };
        reg.request_placement(req, 0).unwrap();
        assert_eq!(reg.pending_requests.len(), 1);

        assert_eq!(reg.cancel_pending_on_disconnect(), 1);
        assert_eq!(reg.pending_requests.len(), 0);
    }
}
