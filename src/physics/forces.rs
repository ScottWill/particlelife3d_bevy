use bevy::{input::common_conditions::input_just_pressed, prelude::*};
use rand::random;
use std::{fmt::{Debug, Display, Formatter, Result}, ops::Index};

use crate::{camera_input_enabled, settings_panel::SimulationConfig, traits::{NextVariant, PrevVariant}};

pub struct ForceMatrixPlugin;

impl Plugin for ForceMatrixPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_force_matrix);
        app.init_resource::<SavedForceMatrix>();
        app.init_resource::<F9HoldTimer>();
        app.add_systems(Update, (
            next_force.run_if(input_just_pressed(KeyCode::BracketRight)),
            prev_force.run_if(input_just_pressed(KeyCode::BracketLeft)),
            negate_bodies.run_if(input_just_pressed(KeyCode::KeyN)),
            shift_left.run_if(input_just_pressed(KeyCode::ArrowDown)),
            shift_right.run_if(input_just_pressed(KeyCode::ArrowUp)),
            shift_down.run_if(input_just_pressed(KeyCode::ArrowLeft)),
            shift_up.run_if(input_just_pressed(KeyCode::ArrowRight)),
            save_forces.run_if(input_just_pressed(KeyCode::F4)),
            mirror.run_if(input_just_pressed(KeyCode::KeyM)),
            symmetry.run_if(input_just_pressed(KeyCode::KeyX)),
        ).run_if(camera_input_enabled));
        app.add_systems(Update, (
            load_forces_on_hold,
        ));
    }
}

fn setup_force_matrix(mut commands: Commands, config: Res<SimulationConfig>) {
    commands.insert_resource(ForceMatrix::new(config.color_count, ForceMatrixType::default()));
}

#[derive(Resource, Default)]
struct SavedForceMatrix(Option<ForceMatrix>);

#[derive(Resource)]
struct F9HoldTimer {
    timer: Timer,
    active: bool,
}

impl Default for F9HoldTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(1.0, TimerMode::Once),
            active: false,
        }
    }
}

fn symmetry(
    mut forces: ResMut<ForceMatrix>,
) {
    forces.make_symmetrical();
}

fn mirror(
    mut forces: ResMut<ForceMatrix>,
) {
    forces.make_mirror();
}

fn save_forces(
    forces: Res<ForceMatrix>,
    mut saved: ResMut<SavedForceMatrix>,
) {
    saved.0 = Some(forces.clone());
}

fn load_forces_on_hold(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut hold: ResMut<F9HoldTimer>,
    mut forces: ResMut<ForceMatrix>,
    saved: Res<SavedForceMatrix>,
) {
    if keys.pressed(KeyCode::F9) {
        hold.timer.tick(time.delta());
        if hold.timer.just_finished() {
            if let Some(ref saved_matrix) = saved.0 {
                *forces = saved_matrix.clone();
            }
        }
    } else {
        if hold.active || hold.timer.elapsed_secs() > 0.0 {
            hold.timer.reset();
        }
        hold.active = false;
    }

    if keys.just_pressed(KeyCode::F9) {
        hold.active = true;
    }
}

fn negate_bodies(
    mut forces: ResMut<ForceMatrix>,
) {
    forces.negate();
}

fn shift_left(
    mut forces: ResMut<ForceMatrix>,
) {
    forces.shift_matrix(ForceShiftType::Row, -1);
}

fn shift_right(
    mut forces: ResMut<ForceMatrix>,
) {
    forces.shift_matrix(ForceShiftType::Row, 1);
}

fn shift_up(
    mut forces: ResMut<ForceMatrix>,
) {
    forces.shift_matrix(ForceShiftType::Column, -1);
}

fn shift_down(
    mut forces: ResMut<ForceMatrix>,
) {
    forces.shift_matrix(ForceShiftType::Column, 1);
}

fn next_force(
    mut forces: ResMut<ForceMatrix>,
) {
    let count = forces.color_count;
    let next = forces.matrix_type.next();

    *forces = ForceMatrix::new(count, next);
}

fn prev_force(
    mut forces: ResMut<ForceMatrix>,
) {
    let count = forces.color_count;
    let prev = forces.matrix_type.prev();

    *forces = ForceMatrix::new(count, prev);
}


#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ForceMatrixType {
    Chains,
    #[default]
    Checkered,
    RandomEx,
    Random,
    Snakes,
    // Symmetry(SymmetryForceMatrix),
    Zeros,
    Ones,
}

// impl Debug for ForceMatrixType {
//     fn fmt(&self, f: &mut Formatter<'_>) -> Result {
//         write!(f, "{}", match &self {
//             ForceMatrixType::Chains => "Chains",
//             ForceMatrixType::Random => "Random",
//             ForceMatrixType::Snakes => "Snakes",
//             ForceMatrixType::Zeros => "Zeros",
//             ForceMatrixType::Ones => "Ones",
//         })
//     }
// }

impl PrevVariant for ForceMatrixType {
    fn prev(&self) -> Self {
        match self {
            ForceMatrixType::Chains    => ForceMatrixType::Checkered,
            ForceMatrixType::Random    => ForceMatrixType::Chains,
            ForceMatrixType::Snakes    => ForceMatrixType::Random,
            ForceMatrixType::Zeros     => ForceMatrixType::Snakes,
            ForceMatrixType::Ones      => ForceMatrixType::Zeros,
            ForceMatrixType::RandomEx  => ForceMatrixType::Ones,
            ForceMatrixType::Checkered => ForceMatrixType::RandomEx,
        }
    }
}

impl NextVariant for ForceMatrixType {
    fn next(&self) -> Self {
        match self {
            ForceMatrixType::Chains    => ForceMatrixType::Random,
            ForceMatrixType::Random    => ForceMatrixType::Snakes,
            ForceMatrixType::Snakes    => ForceMatrixType::Zeros,
            ForceMatrixType::Zeros     => ForceMatrixType::Ones,
            ForceMatrixType::Ones      => ForceMatrixType::RandomEx,
            ForceMatrixType::RandomEx  => ForceMatrixType::Checkered,
            ForceMatrixType::Checkered => ForceMatrixType::Chains,
        }
    }
}

impl ForceMatrixType {
    fn force(self, x: usize, y: usize, w: usize) -> f64 {
        match self {
            ForceMatrixType::Chains    => ChainsForceMatrix::force(x, y, w),
            ForceMatrixType::Random    => RandomForceMatrix::force(x, y, w),
            ForceMatrixType::Snakes    => SnakeForceMatrix::force(x, y, w),
            ForceMatrixType::Zeros     => ZeroForceMatrix::force(x, y, w),
            ForceMatrixType::Ones      => IdentForceMatrix::force(x, y, w),
            ForceMatrixType::RandomEx  => RandomExForceMatrix::force(x, y, w),
            ForceMatrixType::Checkered => CheckeredForceMatrix::force(x, y, w),
        }
    }
}

enum ForceShiftType {
    Column,
    Row,
}

#[derive(Clone, Resource)]
pub struct ForceMatrix {
    pub data: Vec<f64>,
    pub color_count: usize,
    pub matrix_type: ForceMatrixType,
}

impl Display for ForceMatrix {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let forces = self.data
            .chunks(self.color_count)
            .map(|row| {
                row
                    .iter()
                    .map(|v| match v.is_sign_negative() {
                        true => format!("{v:.3}"),
                        false => format!(" {v:.3}"),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join("\n");

        write!(f, "Type: {:?}\nColors: {}\n{forces}\n", self.matrix_type, self.color_count)
    }
}

impl Index<(usize,usize)> for ForceMatrix {
    type Output = f64;

    fn index(&self, (x, y): (usize,usize)) -> &Self::Output {
        match self.get_data(x, y) {
            Some(force) => force,
            None => &0.0,
        }
    }
}

impl ForceMatrix {

    pub fn new(color_count: usize, matrix_type: ForceMatrixType) -> Self {
        assert!(color_count > 0);
        let data = (0..color_count * color_count)
            .into_iter()
            .map(|i| {
                let x = i % color_count;
                let y = i / color_count;
                matrix_type.force(x, y, color_count)
            })
            .collect::<Vec<_>>();

        Self { data, color_count, matrix_type }
    }

    fn make_symmetrical(&mut self) {
        // Build a symmetric Toeplitz matrix from the first row.
        // Value at (i, j) = first_row[min(|i-j|, w - |i-j|)]
        let w = self.color_count;
        let first_row: Vec<f64> = self.data[..w].to_vec();

        for i in 0..w {
            for j in 0..w {
                let diff = if i > j { i - j } else { j - i };
                let dist = diff.min(w - diff);
                let ix = self.data_ix(j, i);
                self.data[ix] = first_row[dist];
            }
        }
    }

    fn make_mirror(&mut self) {
        // Mirror across the diagonal so matrix[i][j] == matrix[j][i].
        // Copies the upper triangle to the lower triangle.
        let w = self.color_count;
        for i in 0..w {
            for j in (i + 1)..w {
                let upper = self.data_ix(j, i);
                let lower = self.data_ix(i, j);
                self.data[lower] = self.data[upper];
            }
        }
    }

    // fn copy_to_clipboard(&self) {
    //     let output = self.data
    //         .chunks_exact(self.color_count)
    //         .map(|chunk| chunk
    //             .into_iter()
    //             .map(|f| f.to_string())
    //             .collect::<Vec<_>>()
    //             .join(",")
    //         )
    //         .collect::<Vec<_>>()
    //         .join("\n");
    //     let mut clipboard = Clipboard::new().unwrap();
    //     clipboard.set_text(output).unwrap();
    // }

    // fn paste_from_clipboard(&mut self) {
    //     let mut clipboard = Clipboard::new().unwrap();
    //     if let Ok(contents) = clipboard.get_text() {
    //         let mut data = Vec::with_capacity(self.color_count * self.color_count);
    //         for line in contents.lines() {
    //             let parts = line.split(',').collect::<Vec<_>>();
    //             for part in parts {
    //                 if let Ok(num) = part.trim().parse::<f64>() {
    //                     data.push(num.into());
    //                 } else {
    //                     break;
    //                 }
    //             }
    //         }
    //         if data.len() == self.data.len() {
    //             self.data = data;
    //         }
    //     }
    // }

    #[inline]
    fn data_ix(&self, x: usize, y: usize) -> usize {
        x + y * self.color_count
    }

    #[inline]
    fn get_data(&self, x: usize, y: usize) -> Option<&f64> {
        let ix = self.data_ix(x, y);
        self.data.get(ix)
    }

    // #[inline]
    // pub fn get_force(&self, x: usize, y: usize) -> f64 {
    //     match self.get_data(x, y) {
    //         Some(force) => *force,
    //         None => 0.0,
    //     }
    // }

    // fn abs(&mut self) {
    //     for cell in &mut self.data {
    //         *cell = cell.abs();
    //     }
    // }

    pub fn negate(&mut self) {
        for cell in &mut self.data {
            *cell *= -1.0;
        }
    }

    // pub fn expand(&mut self) {
    //     let new_size = self.color_count + 1;
    //     self.data = (0..new_size * new_size)
    //         .into_iter()
    //         .map(|i| {
    //             let x = i % new_size;
    //             let y = i / new_size;
    //             match self.get_data(x, y) {
    //                 Some(cell) => cell.clone(),
    //                 None => {
    //                     let f = match self.matrix_type {
    //                         ForceMatrixType::Chains(p) => p.force(x, y, new_size),
    //                         ForceMatrixType::Random(p) => p.force(x, y, new_size),
    //                         ForceMatrixType::Snakes(p) => p.force(x, y, new_size),
    //                         ForceMatrixType::Zeros(p) => p.force(x, y, new_size),
    //                         ForceMatrixType::Ones(p) => p.force(x, y, new_size),
    //                     };
    //                     f.into()
    //                 },
    //             }
    //         })
    //         .collect::<Vec<_>>();
    //     self.color_count = new_size;
    // }

    // pub fn shrink(&mut self) {
    //     if self.color_count > 1 {
    //         let new_len = self.data.len() - self.color_count;
    //         self.color_count -= 1;
    //         self.data = self.data[0..new_len]
    //             .chunks_exact(self.color_count + 1)
    //             .into_iter()
    //             .flat_map(|chunks|
    //                 chunks
    //                     .iter()
    //                     .take(self.color_count)
    //                     .map(|cell| cell.clone())
    //             )
    //             .collect::<Vec<_>>();
    //     }
    // }

    fn shift_matrix(&mut self, shift_type: ForceShiftType, amount: isize) {
        self.data = self.data
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let (x, y) = match shift_type {
                    ForceShiftType::Column => (
                        (((i % self.color_count) as isize) + amount).rem_euclid(self.color_count as isize) as usize,
                        i / self.color_count
                    ),
                    ForceShiftType::Row => (
                        i % self.color_count,
                        (((i / self.color_count) as isize) + amount).rem_euclid(self.color_count as isize) as usize
                    ),
                };
                self[(x, y)]
            })
            .collect();
    }

    // pub fn force_matrix_ui(&mut self, ui: &mut Ui, config: &mut ConfigState) {
    //     ui.horizontal(|ui| {
    //         if ui.button(" < ").clicked() {
    //             self.shift_matrix(ForceShiftType::Column, 1);
    //         }
    //         if ui.button(" > ").clicked() {
    //             self.shift_matrix(ForceShiftType::Column, -1);
    //         }
    //         if ui.button(" ⬆ ").clicked() {
    //             self.shift_matrix(ForceShiftType::Row, 1);
    //         }
    //         if ui.button(" ⬇ ").clicked() {
    //             self.shift_matrix(ForceShiftType::Row, -1);
    //         }
    //         if ui.button(" Abs ").clicked() {
    //             self.abs();
    //         }
    //         if ui.button(" Neg ").clicked() {
    //             self.negate();
    //         }
    //         if ui.button(" Copy ").clicked() {
    //             self.copy_to_clipboard();
    //         }
    //         if ui.button(" Paste ").clicked() {
    //             self.paste_from_clipboard();
    //         }
    //     });
    //     // todo: force matric color boxes
    //     egui::ScrollArea::both()
    //         .max_height(300.0)
    //         .show(ui, |ui| {
    //             egui::Grid::new("force_matrix")
    //                 .spacing([1.0, 1.0])
    //                 .striped(true)
    //                 .show(ui, |ui| {
    //                     for y in 0..self.color_count {
    //                         let y = y * self.color_count;
    //                         for x in 0..self.color_count {
    //                             if let Some(cell) = self.data.get_mut(x + y) {
    //                                 ui.add(DragValue::new(cell).speed(0.001));
    //                             }
    //                         }
    //                         ui.end_row();
    //                     }
    //                 });
    //         });

    //     // forces select
    //     ui.horizontal(|ui| {
    //         if ui.button(" Update ").clicked() {
    //             *self = ForceMatrix::new(config.colors_count as usize, config.force_matrix_option);
    //         }
    //         egui::ComboBox::from_label("Matrix")
    //             .selected_text(format!("{:?}", config.force_matrix_option))
    //             .show_ui(ui, |ui| {
    //                 // ui.style_mut().wrap = Some(false);
    //                 ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
    //                 ui.set_min_width(60.0);
    //                 for f in ForceMatrixType::iter() {
    //                     ui.selectable_value(&mut config.force_matrix_option, f, format!("{f}"));
    //                 }
    //             });
    //         ui.end_row();
    //     });

    // }

}

trait MatrixProvider {
    fn force(x: usize, y: usize, w: usize) -> f64;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChainsForceMatrix;
impl MatrixProvider for ChainsForceMatrix {
    fn force(x: usize, y: usize, w: usize) -> f64 {
        let amt = 1.0;
        match (y, x) {
            (y, x) if y == x => amt,
            (y, x) if y == (x + 1) % w => amt,
            (y, x) if y == (x + w - 1) % w => amt,
            _ => 0.0
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CheckeredForceMatrix;
impl MatrixProvider for CheckeredForceMatrix {
    fn force(x: usize, y: usize, _: usize) -> f64 {
        if x % 2 == y % 2 { -1.0 } else { 1.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RandomForceMatrix;
impl MatrixProvider for RandomForceMatrix {
    fn force(_: usize, _: usize, _: usize) -> f64 {
        random::<f64>() * 2.0 - 1.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RandomExForceMatrix;
impl MatrixProvider for RandomExForceMatrix {
    fn force(x: usize, y: usize, _: usize) -> f64 {
        if x == y { 0.0 } else { random::<f64>() * 2.0 - 1.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SnakeForceMatrix;
impl MatrixProvider for SnakeForceMatrix {
    fn force(x: usize, y: usize, w: usize) -> f64 {
        match (y, x) {
            (y, x) if y == x => 1.0,
            (y, x) if y == (x + 1) % w => 0.2,
            _ => 0.0,
        }
    }
}

// #[derive(Clone, Copy, Default, Debug, PartialEq)]
// pub struct SymmetryForceMatrix(pub usize);
// impl MatrixProvider for SymmetryForceMatrix {
//     fn force(self, _x: usize, _y: usize, _w: usize) -> f64 {
//         todo!()
//     }
// }

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct ZeroForceMatrix;
impl MatrixProvider for ZeroForceMatrix {
    fn force(_: usize, _: usize, _: usize) -> f64 {
        0.0
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct IdentForceMatrix;
impl MatrixProvider for IdentForceMatrix {
    fn force(_: usize, _: usize, _: usize) -> f64 {
        1.0
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_force_matrix_type() -> impl Strategy<Value = ForceMatrixType> {
        prop_oneof![
            Just(ForceMatrixType::Chains),
            Just(ForceMatrixType::Checkered),
            Just(ForceMatrixType::RandomEx),
            Just(ForceMatrixType::Random),
            Just(ForceMatrixType::Snakes),
            Just(ForceMatrixType::Zeros),
            Just(ForceMatrixType::Ones),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        // Feature: egui-settings-panel, Property 2: Force matrix dimension invariant
        // **Validates: Requirements 9.2, 10.3**
        #[test]
        fn force_matrix_dimension_invariant(
            color_count in 1usize..=9,
            matrix_type in arb_force_matrix_type(),
        ) {
            let matrix = ForceMatrix::new(color_count, matrix_type);
            prop_assert_eq!(matrix.data.len(), color_count * color_count);
            for &cell in &matrix.data {
                prop_assert!(cell >= -1.0 && cell <= 1.0,
                    "Cell value {} out of range [-1.0, 1.0]", cell);
            }
        }

        // Feature: egui-settings-panel, Property 3: Force matrix display completeness
        // **Validates: Requirements 3.2, 10.4**
        #[test]
        fn force_matrix_display_completeness(
            color_count in 1usize..=9,
            matrix_type in arb_force_matrix_type(),
        ) {
            let matrix = ForceMatrix::new(color_count, matrix_type);
            let display = format!("{}", matrix);

            // Must contain the matrix type name
            let type_name = format!("{:?}", matrix_type);
            prop_assert!(display.contains(&type_name),
                "Display should contain type name '{}', got: {}", type_name, display);

            // Must contain color count
            let color_str = format!("Colors: {}", color_count);
            prop_assert!(display.contains(&color_str),
                "Display should contain '{}', got: {}", color_str, display);

            // Count values by splitting the data lines and counting formatted floats
            // The format is: "Type: ...\nColors: N\nrow0\nrow1\n...\n"
            let lines: Vec<&str> = display.lines().collect();
            // First line is "Type: ..." and second is "Colors: ..."
            // Remaining lines are data rows
            let data_lines = &lines[2..];
            prop_assert_eq!(data_lines.len(), color_count,
                "Expected {} data rows, found {}", color_count, data_lines.len());

            let mut total_values = 0;
            for line in data_lines {
                // Count comma-separated values in each row
                let values: Vec<&str> = line.split(',').collect();
                prop_assert_eq!(values.len(), color_count,
                    "Expected {} values per row, found {}", color_count, values.len());
                total_values += values.len();

                // Verify each value has 3 decimal places
                for val in &values {
                    let trimmed = val.trim();
                    prop_assert!(trimmed.contains('.'),
                        "Value '{}' should contain decimal point", trimmed);
                    let decimals = trimmed.split('.').last().unwrap();
                    prop_assert_eq!(decimals.len(), 3,
                        "Value '{}' should have 3 decimal places, has {}", trimmed, decimals.len());
                }
            }
            prop_assert_eq!(total_values, color_count * color_count,
                "Expected {} total values, found {}", color_count * color_count, total_values);
        }
    }
}
