// 15-second section strain peaks for miss weighting.
//
// Similar to local_sr_per_minute but bins strain peaks into 15-second sections
// instead of 1-minute sections. Used for analyzing difficulty variations
// within shorter time windows for more granular miss weight estimation.

use super::marathon::{difficulty_value_from_peaks, star_from_aim_speed};

const PEAK_SECTION_LEN_MS: f64 = 400.0;
const SECTION_MS: f64 = 15_000.0;

/// Bin raw 400 ms strain peaks into per-15-second local star ratings.
///
/// Returns a vector where each element represents the star rating for a 15-second
/// section of the map. This provides more granular difficulty analysis compared
/// to the 1-minute binning used in local_sr_per_minute.
pub(crate) fn local_sr_per_15s(strains_aim: &[f64], strains_speed: &[f64]) -> Vec<f64> {
    let peaks_per_section = (SECTION_MS / PEAK_SECTION_LEN_MS).round() as usize; // ~37-38
    let len = strains_aim.len().min(strains_speed.len());
    if len == 0 {
        return Vec::new();
    }
    let n_sections = (len + peaks_per_section - 1) / peaks_per_section;

    let mut out = Vec::with_capacity(n_sections);
    for k in 0..n_sections {
        let start = k * peaks_per_section;
        let end = ((k + 1) * peaks_per_section).min(len);
        let aim_slice = &strains_aim[start..end];
        let speed_slice = &strains_speed[start..end];
        out.push(star_from_aim_speed(aim_slice, speed_slice));
    }
    out
}
