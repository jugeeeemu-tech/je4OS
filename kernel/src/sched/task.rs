//! タスク構造体と関連型の定義
//!
//! このモジュールはタスクの基本的な構造体、状態、優先度を定義します。

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::paging::KERNEL_VIRTUAL_BASE;

use super::context::Context;

/// タスク操作のエラー型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskError {
    /// 無効な優先度（アイドルタスク以下）
    InvalidPriority,
    /// スタック割り当て失敗
    StackAllocationFailed,
    /// 無効なスタックアドレス
    InvalidStackAddress,
    /// コンテキスト初期化失敗
    ContextInitFailed,
    /// タスクキューが満杯
    QueueFull,
}

impl core::fmt::Display for TaskError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            TaskError::InvalidPriority => {
                write!(f, "Invalid task priority (must be >= {})", priority::MIN)
            }
            TaskError::StackAllocationFailed => write!(f, "Failed to allocate task stack"),
            TaskError::InvalidStackAddress => write!(f, "Invalid stack address"),
            TaskError::ContextInitFailed => write!(f, "Failed to initialize task context"),
            TaskError::QueueFull => write!(f, "Task queue is full"),
        }
    }
}

/// タスクID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(u64);

impl TaskId {
    /// 新しいタスクIDを生成
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        TaskId(id)
    }

    /// タスクIDの値を取得
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// タスク優先度の定数
pub mod priority {
    /// アイドルタスクの優先度（最低）
    pub const IDLE: u8 = 0;
    /// 通常タスクの最低優先度
    pub const MIN: u8 = 1;
    /// デフォルト優先度
    pub const DEFAULT: u8 = 10;
    /// 最高優先度
    pub const MAX: u8 = 255;
    /// リアルタイムクラスの下限（この値以上はリアルタイムクラス）
    pub const RT_MIN: u8 = 100;
}

/// スケジューリングクラス
///
/// タスクの優先度クラスを表します。上位クラスのキューが空になるまで、
/// 下位クラスのタスクは実行されません。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingClass {
    /// リアルタイムクラス（最高優先度）
    /// Compositor、マウス描画など即座に応答が必要なタスク用
    Realtime = 2,
    /// 通常クラス（CFS方式）
    /// 一般的なアプリケーションタスク用
    Normal = 1,
    /// アイドルクラス（最低優先度）
    /// 他に実行可能タスクがない場合のみ実行
    Idle = 0,
}

/// 優先度からスケジューリングクラスを判定
///
/// # Arguments
/// * `priority` - タスクの優先度 (0-255)
///
/// # Returns
/// 対応するスケジューリングクラス
pub fn priority_to_class(priority: u8) -> SchedulingClass {
    if priority == priority::IDLE {
        SchedulingClass::Idle
    } else if priority >= priority::RT_MIN {
        SchedulingClass::Realtime
    } else {
        SchedulingClass::Normal
    }
}

/// 優先度から重みを計算
///
/// 優先度が高いタスクほど大きな重みを持ち、vruntimeの増加が遅くなります。
/// - 基準優先度（DEFAULT=10）の重みは1024
/// - 優先度が1上がるごとに約1.25倍の重みを持つ
///
/// # Arguments
/// * `priority` - タスクの優先度（0-255）
///
/// # Returns
/// スケジューリング用の重み
pub fn priority_to_weight(priority: u8) -> u32 {
    const PRIO_TO_WEIGHT: [u32; 40] = [
        88761, 71755, 56483, 46273, 36291, 29154, 23254, 18705, 14949, 11916, 9548, 7620, 6100,
        4904, 3906, 3121, 2501, 1991, 1586, 1277, 1024, 820, 655, 526, 423, 335, 272, 215, 172,
        137, 110, 87, 70, 56, 45, 36, 29, 23, 18, 15,
    ];

    // 0-255 の優先度を 0-39 のインデックスに変換
    // priority 0 (低) -> index 39 (重み15)
    // priority 255 (高) -> index 0 (重み88761)

    // 単純に範囲を圧縮する
    let index = 39 - (priority as usize * 40 / 256).min(39);

    PRIO_TO_WEIGHT[index]
}

/// タスクの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// 実行中（CPUを使用中）
    Running,
    /// 実行可能（CPUを待機中）
    Ready,
    /// ブロック中（I/O待ちなど）
    Blocked,
    /// 終了
    Terminated,
}

/// タスクスタック
const STACK_SIZE: usize = 16384; // 16KB

#[repr(align(16))]
pub(super) struct TaskStack([u8; STACK_SIZE]);

impl TaskStack {
    pub(super) const fn new() -> Self {
        Self([0; STACK_SIZE])
    }

    /// スタックの最上位アドレスを取得（仮想アドレス）
    pub(super) fn top(&self) -> u64 {
        let base = self.0.as_ptr() as u64;
        let physical_top = base + STACK_SIZE as u64;

        // ヒープは物理アドレスで割り当てられるため、カーネル仮想アドレスに変換
        // カーネルは KERNEL_VIRTUAL_BASE (0xFFFF800000000000) 以降で動作
        if physical_top >= KERNEL_VIRTUAL_BASE {
            // 既に仮想アドレス
            physical_top
        } else {
            // 物理アドレスを仮想アドレスに変換
            KERNEL_VIRTUAL_BASE + physical_top
        }
    }
}

/// タスク制御ブロック (Task Control Block)
pub struct Task {
    /// タスクID
    id: TaskId,
    /// タスク名（デバッグ用）
    name: &'static str,
    /// タスク優先度（0-255、数値が大きいほど優先度が高い）
    priority: u8,
    /// スケジューリングクラス（Realtime, Normal, Idle）
    sched_class: SchedulingClass,
    /// スケジューリング用の重み（優先度から計算、Normalクラスで使用）
    /// 値が大きいほど、vruntimeの増加が遅くなり、より頻繁に実行される
    weight: u32,
    /// 仮想実行時間（CFS風スケジューリング、Normalクラスで使用）
    /// この値が小さいタスクが優先的に実行される
    vruntime: u64,
    /// CPUコンテキスト
    context: Context,
    /// タスクの状態
    state: TaskState,
    /// タスク専用スタック（ヒープに割り当て）
    stack: Box<TaskStack>,
}

impl Task {
    /// 新しいタスクを作成
    ///
    /// # Arguments
    /// * `name` - タスク名
    /// * `priority` - タスク優先度（priority::MIN以上、priority::MAX以下）
    /// * `entry_point` - エントリポイント関数のアドレス
    ///
    /// # Errors
    /// * `TaskError::InvalidPriority` - 優先度がpriority::MINより小さい場合
    /// * `TaskError::StackAllocationFailed` - スタック割り当てに失敗した場合
    /// * `TaskError::ContextInitFailed` - コンテキスト初期化に失敗した場合
    pub fn new(
        name: &'static str,
        priority: u8,
        entry_point: extern "C" fn() -> !,
    ) -> Result<Self, TaskError> {
        // 優先度の検証：アイドルタスク以下の優先度は許可しない
        if priority < priority::MIN {
            return Err(TaskError::InvalidPriority);
        }

        // スタックをヒープに割り当て
        let stack = Box::new(TaskStack::new());
        let stack_top = stack.top();

        let context = Context::new(entry_point as u64, stack_top)?;

        // 優先度から重みとスケジューリングクラスを計算
        let weight = priority_to_weight(priority);
        let sched_class = priority_to_class(priority);

        Ok(Self {
            id: TaskId::new(),
            name,
            priority,
            sched_class,
            weight,
            vruntime: 0, // 初期値は0
            context,
            state: TaskState::Ready,
            stack,
        })
    }

    /// アイドルタスク専用の作成関数（優先度チェックをスキップ）
    ///
    /// # Arguments
    /// * `name` - タスク名
    /// * `entry_point` - エントリポイント関数のアドレス
    ///
    /// # Errors
    /// * `TaskError::StackAllocationFailed` - スタック割り当てに失敗した場合
    /// * `TaskError::ContextInitFailed` - コンテキスト初期化に失敗した場合
    pub fn new_idle(
        name: &'static str,
        entry_point: extern "C" fn() -> !,
    ) -> Result<Self, TaskError> {
        // スタックをヒープに割り当て
        let stack = Box::new(TaskStack::new());
        let stack_top = stack.top();

        let context = Context::new(entry_point as u64, stack_top)?;

        // アイドルタスクの重みを計算
        let weight = priority_to_weight(priority::IDLE);

        Ok(Self {
            id: TaskId::new(),
            name,
            priority: priority::IDLE,
            sched_class: SchedulingClass::Idle, // アイドルタスクは常にIdleクラス
            weight,
            vruntime: 0, // 初期値は0
            context,
            state: TaskState::Ready,
            stack,
        })
    }

    /// タスクIDを取得
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// タスク名を取得
    pub fn name(&self) -> &str {
        self.name
    }

    /// タスク優先度を取得
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// スケジューリングクラスを取得
    pub fn sched_class(&self) -> SchedulingClass {
        self.sched_class
    }

    /// タスクの重みを取得
    pub fn weight(&self) -> u32 {
        self.weight
    }

    /// タスクの仮想実行時間を取得
    pub fn vruntime(&self) -> u64 {
        self.vruntime
    }

    /// タスクの仮想実行時間を更新
    ///
    /// # Arguments
    /// * `delta` - 実際の実行時間（ナノ秒単位）
    pub fn update_vruntime(&mut self, delta: u64) {
        // vruntime += delta * BASE_WEIGHT / weight
        // 優先度が高い（weightが大きい）ほど、vruntimeの増加が遅い
        const BASE_WEIGHT: u64 = 1024;

        if self.weight == 0 {
            return;
        }

        let increment = (delta * BASE_WEIGHT) / (self.weight as u64);
        self.vruntime = self.vruntime.saturating_add(increment);
    }

    /// タスクの状態を取得
    pub fn state(&self) -> TaskState {
        self.state
    }

    /// タスクの状態を設定
    pub fn set_state(&mut self, state: TaskState) {
        self.state = state;
    }

    /// コンテキストへの参照を取得
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// コンテキストへの可変参照を取得
    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }
}
