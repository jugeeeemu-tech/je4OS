//! Wait Queue - ブロックされたタスクを管理するキュー

use crate::task::TaskId;
use alloc::collections::VecDeque;
use spin::Mutex as SpinMutex;

/// ブロックされたタスクを管理するキュー
pub struct WaitQueue {
    /// 待機中のタスクIDリスト
    waiters: SpinMutex<VecDeque<TaskId>>,
}

impl WaitQueue {
    /// 新しいWaitQueueを作成
    pub const fn new() -> Self {
        Self {
            waiters: SpinMutex::new(VecDeque::new()),
        }
    }

    /// 現在のタスクを待機キューに追加してブロック
    ///
    /// この関数はタスクをBlockedに設定してスケジュールします。
    pub fn wait(&self) {
        let task_id = crate::task::current_task_id();
        {
            let mut waiters = self.waiters.lock();
            waiters.push_back(task_id);
        }
        // タスクをBlockedに設定してスケジュール
        crate::task::block_current_task();
    }

    /// 1つのタスクを起床させる
    ///
    /// # Returns
    /// 起床させたタスクがあればtrue、キューが空ならfalse
    pub fn wake_one(&self) -> bool {
        let mut waiters = self.waiters.lock();
        if let Some(task_id) = waiters.pop_front() {
            drop(waiters); // ロックを早期に解放
            crate::task::unblock_task(task_id);
            true
        } else {
            false
        }
    }

    /// すべてのタスクを起床させる
    pub fn wake_all(&self) {
        let mut waiters = self.waiters.lock();
        while let Some(task_id) = waiters.pop_front() {
            // 各タスクを起床させる前にロックを一時的に解放
            drop(waiters);
            crate::task::unblock_task(task_id);
            waiters = self.waiters.lock();
        }
    }
}
