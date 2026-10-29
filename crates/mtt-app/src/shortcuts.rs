//! Keyboard shortcuts as the user sees them: one list for the shortcut sheet (`?`) and the
//! settings tab, and the platform spelling of modifier keys. Keys are written the macOS
//! way (`⌘`, `⌥`, `⇧`, `⌫`) and spelled out elsewhere.

use eframe::egui::{self, RichText};

use crate::theme;

pub struct Group {
    pub title: &'static str,
    pub items: &'static [(&'static str, &'static str)],
}

pub const GROUPS: &[Group] = &[
    Group {
        title: "모드 · 화면",
        items: &[
            ("2", "번역 모드"),
            ("3 / 4", "인페인트 결과 / 마스크"),
            ("1", "검수 · 출력"),
            ("5", "원본 이미지 보기 켜고 끄기"),
            ("Space", "누르는 동안 최종 결과 보기 · 끌면 이동"),
            ("스크롤", "화면 이동"),
            ("⌘ 스크롤", "확대 / 축소 (트랙패드: 벌리기)"),
            ("⌘0", "화면에 맞춤"),
            ("⌘+ / ⌘-", "확대 / 축소"),
        ],
    },
    Group {
        title: "페이지 · 블록",
        items: &[
            ("D / F", "이전 / 다음 페이지 (← → 도 됨)"),
            ("Tab / ⇧Tab", "다음 / 이전 텍스트 블록"),
            ("₩", "이 페이지 검수 완료 표시 (` 도 됨)"),
            ("⌘A", "블록 모두 선택"),
            ("⇧ 클릭", "블록 여러 개 선택"),
            ("?", "이 단축키 표 열고 닫기"),
        ],
    },
    Group {
        title: "편집",
        items: &[
            ("E", "문장 바로 고치기 (더블클릭도 됨)"),
            ("⌘Enter / Esc", "바로 고치기 확정 / 취소"),
            ("⌘Z / ⇧⌘Z", "실행 취소 / 다시 실행"),
            ("⌘C / ⌘V", "블록 복사 / 붙여넣기"),
            ("⌘D", "블록 복제 (⌘ 클릭도 됨)"),
            ("⌫", "선택 삭제 (한손모드는 Q)"),
            ("⌥ 드래그", "선택한 블록의 영역 다시 그리기"),
            ("⌘F", "화 전체에서 찾아 바꾸기"),
            ("⌘S", "바로 저장"),
        ],
    },
    Group {
        title: "도구",
        items: &[
            ("A", "선택 (인페인트: 화면 이동)"),
            ("T", "범위 선택"),
            ("Z", "확대 도구 (⌥ 클릭은 축소)"),
            ("Esc", "확대 · 범위 도구 끝내기"),
            ("B", "브러시"),
            ("J", "스마트 브러시 (칠한 곳 메우기)"),
            ("E", "지우개 (인페인트 모드)"),
            ("⌥E", "덩어리 지우개 (마스크)"),
            ("I", "스포이드 (결과)"),
            ("/", "마스크 유지 켜고 끄기 (결과)"),
            ("[ / ]", "브러시 크기"),
        ],
    },
];

/// `mac` spelled for this platform: `⌘` → `Ctrl+`, `⌥` → `Alt+`, `⇧` → `Shift+`,
/// `⌫` → `Delete` outside macOS.
pub fn keys(mac: &str) -> String {
    if cfg!(target_os = "macos") {
        return mac.to_owned();
    }
    mac.replace('⌘', "Ctrl+").replace('⌥', "Alt+").replace('⇧', "Shift+").replace('⌫', "Delete")
}

/// Key names as small keycaps.
pub fn keycaps(ui: &mut egui::Ui, mac: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (index, part) in keys(mac).split(" / ").enumerate() {
            if index > 0 {
                ui.label(RichText::new("/").small().color(theme::TEXT_WEAK));
            }
            egui::Frame::new()
                .fill(theme::INPUT_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(5, 1))
                .show(ui, |ui| ui.label(RichText::new(part).size(11.5).color(theme::TEXT)));
        }
    });
}

/// Every group as a grid of keycaps and descriptions, in `columns` columns; each group goes
/// into the column that is shortest so far.
pub fn sheet(ui: &mut egui::Ui, columns: usize) {
    let columns = columns.max(1);
    let mut placement: Vec<Vec<usize>> = vec![Vec::new(); columns];
    let mut rows = vec![0; columns];
    for (index, group) in GROUPS.iter().enumerate() {
        let column = (0..columns).min_by_key(|&column| rows[column]).unwrap_or(0);
        rows[column] += group.items.len() + 2;
        placement[column].push(index);
    }
    ui.columns(columns, |cols| {
        for (ui, groups) in cols.iter_mut().zip(&placement) {
            for &index in groups {
                let group = &GROUPS[index];
                ui.label(theme::semibold(group.title));
                ui.add_space(2.0);
                let grid = egui::Grid::new(("shortcut-group", index)).num_columns(2).spacing([10.0, 3.0]);
                grid.min_row_height(20.0).show(ui, |ui| {
                    for (mac, action) in group.items {
                        keycaps(ui, mac);
                        ui.label(RichText::new(*action).small());
                        ui.end_row();
                    }
                });
                ui.add_space(10.0);
            }
        }
    });
}

/// Columns for the shortcut sheet in a window `width` points wide, and the sheet's width.
pub fn sheet_layout(width: f32) -> (usize, f32) {
    if width >= 1040.0 { (3, (width - 60.0).min(1120.0)) } else { (2, (width - 60.0).min(760.0)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_follow_the_platform() {
        if cfg!(target_os = "macos") {
            assert_eq!(keys("⇧⌘Z"), "⇧⌘Z");
        } else {
            assert_eq!(keys("⇧⌘Z"), "Shift+Ctrl+Z");
            assert_eq!(keys("⌥B"), "Alt+B");
        }
    }
}
