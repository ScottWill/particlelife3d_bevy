use bevy::math::DVec3;
use bevy::prelude::*;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;

use crate::physics::{ForceBackend, PointPosition};

const MAX_HISTORY: usize = 1_000;

#[derive(Component)]
pub struct TrackedParticle;

#[derive(Clone, Debug)]
pub struct TrackedSample {
    pub time: f64,
    pub position: DVec3,
}

#[derive(Default, Resource)]
pub struct ParticleTracker {
    pub samples: VecDeque<TrackedSample>,
    pub recording_complete: bool,
}

impl ParticleTracker {
    pub fn clear(&mut self) {
        self.samples.clear();
        self.recording_complete = false;
    }
}

pub struct TrackingPlugin;

impl Plugin for TrackingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticleTracker>();
        app.add_systems(FixedPostUpdate, (
            record_tracked_particle,
            save_and_exit.run_if(|tracker: Res<ParticleTracker>| tracker.recording_complete),
        ));
    }
}

fn record_tracked_particle(
    mut tracker: ResMut<ParticleTracker>,
    position: Single<&PointPosition, With<TrackedParticle>>,
    time: Res<Time>,
) {
    if tracker.recording_complete {
        return;
    }

    tracker.samples.push_back(TrackedSample {
        time: time.elapsed_secs_f64(),
        position: position.0,
    });

    if tracker.samples.len() >= MAX_HISTORY {
        tracker.recording_complete = true;
    }
}

fn save_and_exit(
    tracker: Res<ParticleTracker>,
    backend: Res<ForceBackend>,
    mut exit: MessageWriter<AppExit>,
) {
    let filename = format!("{}_{}.csv", *backend, std::process::id());
    if let Ok(mut file) = File::create(&filename) {
        writeln!(file, "time,x,y,z").ok();
        for sample in tracker.samples.iter() {
            writeln!(
                file,
                "{},{},{},{}",
                sample.time, sample.position.x, sample.position.y, sample.position.z
            ).ok();
        }
        info!("Saved {} samples to {}", tracker.samples.len(), filename);
    } else {
        error!("Failed to create tracking file: {}", filename);
    }
    exit.write(AppExit::Success);
}
