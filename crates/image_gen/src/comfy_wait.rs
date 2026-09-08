//! ComfyUI の待機判定 (契約 `ImageGenConfig.comfy_wait`、rev2 査読 7)。純粋部分。
//!
//! Kataribe は `/history/{id}` を 1 秒間隔で見るだけで、404 の期間が『実行待ち』か『消失』か区別できず
//! 600 秒待ってから Timeout になっていた。ここでは `/queue` を併読し、どこにも無ければ即失敗にする。
//! `/queue` の形 (ComfyUI server.py): `{"queue_running": [[number, prompt_id, prompt, extra, outputs], ...],
//! "queue_pending": [...]}` — **実機未確認** (Phase C live で固定する)。

use std::time::Duration;

use serde_json::Value;

/// ポーリング間隔 (契約: 1s → 2s → 4s → 上限 5s)。
pub fn backoff(attempt: u32) -> Duration {
    let secs = match attempt {
        0 => 1,
        1 => 2,
        2 => 4,
        _ => 5,
    };
    Duration::from_secs(secs)
}

/// `/queue` 上での我々の prompt_id の状態。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueState {
    Running,
    /// 前に何件並んでいるか。
    Pending { ahead: usize },
    /// running にも pending にも無い (history にも無ければ消失)。
    Absent,
}

fn item_prompt_id(item: &Value) -> Option<&str> {
    item.as_array().and_then(|a| a.get(1)).and_then(Value::as_str)
}

/// `/queue` の応答から状態を読む (純粋)。形が違えば `Absent` ではなく `None` (判断不能 = 待機継続)。
pub fn queue_state(queue: &Value, prompt_id: &str) -> Option<QueueState> {
    let running = queue.get("queue_running")?.as_array()?;
    let pending = queue.get("queue_pending")?.as_array()?;
    if running.iter().any(|i| item_prompt_id(i) == Some(prompt_id)) {
        return Some(QueueState::Running);
    }
    if let Some(pos) = pending.iter().position(|i| item_prompt_id(i) == Some(prompt_id)) {
        return Some(QueueState::Pending { ahead: pos });
    }
    Some(QueueState::Absent)
}

/// 進捗文言 (UI 用)。
pub fn describe(state: &QueueState) -> String {
    match state {
        QueueState::Running => "ComfyUI: 生成中".into(),
        QueueState::Pending { ahead } => format!("ComfyUI: 待機中 (前に {ahead} 件)"),
        QueueState::Absent => "ComfyUI: キューに見つかりません".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn backoff_grows_then_caps_at_five_seconds() {
        let s: Vec<u64> = (0..6).map(|a| backoff(a).as_secs()).collect();
        assert_eq!(s, vec![1, 2, 4, 5, 5, 5]);
    }

    #[test]
    fn queue_state_reads_running_pending_and_absent() {
        let q = json!({
            "queue_running": [[7, "run-1", {}, {}, []]],
            "queue_pending": [[8, "p-a", {}, {}, []], [9, "p-b", {}, {}, []]]
        });
        assert_eq!(queue_state(&q, "run-1"), Some(QueueState::Running));
        assert_eq!(queue_state(&q, "p-b"), Some(QueueState::Pending { ahead: 1 }));
        assert_eq!(queue_state(&q, "gone"), Some(QueueState::Absent));
    }

    #[test]
    fn malformed_queue_is_undecidable_not_absent() {
        assert_eq!(queue_state(&json!({"nope": 1}), "x"), None);
        assert_eq!(queue_state(&json!({"queue_running": 3, "queue_pending": []}), "x"), None);
    }

    #[test]
    fn describe_is_human_readable() {
        assert_eq!(describe(&QueueState::Pending { ahead: 2 }), "ComfyUI: 待機中 (前に 2 件)");
    }
}
