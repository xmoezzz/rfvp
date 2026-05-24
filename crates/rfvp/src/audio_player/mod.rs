// Real kira-based players (desktop with audio feature)
#[cfg(all(feature = "audio", not(target_os = "uefi")))]
pub mod bgm_player;
// Anzu-HAL players (UEFI with anzu-audio feature)
#[cfg(all(target_os = "uefi", feature = "anzu-audio"))]
#[path = "bgm_player_anzu.rs"]
pub mod bgm_player;
// No-audio stub (UEFI without anzu-audio, or desktop no-audio builds)
#[cfg(any(
    all(not(feature = "audio"), not(target_os = "uefi")),
    all(target_os = "uefi", not(feature = "anzu-audio")),
))]
#[path = "bgm_player_no_audio.rs"]
pub mod bgm_player;

// Real kira-based players (desktop with audio feature)
#[cfg(all(feature = "audio", not(target_os = "uefi")))]
pub mod se_player;
// Anzu-HAL players (UEFI with anzu-audio feature)
#[cfg(all(target_os = "uefi", feature = "anzu-audio"))]
#[path = "se_player_anzu.rs"]
pub mod se_player;
// No-audio stub (UEFI without anzu-audio, or desktop no-audio builds)
#[cfg(any(
    all(not(feature = "audio"), not(target_os = "uefi")),
    all(target_os = "uefi", not(feature = "anzu-audio")),
))]
#[path = "se_player_no_audio.rs"]
pub mod se_player;

pub use bgm_player::{BgmPlayer, BgmPlayerSnapshotV1, BgmSlotSnapshotV1, BGM_SLOT_COUNT};
pub use se_player::{SePlayer, SePlayerSnapshotV1, SeSlotSnapshotV1, SE_SLOT_COUNT};
