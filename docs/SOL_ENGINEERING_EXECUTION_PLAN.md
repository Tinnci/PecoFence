# PecoFence 与 SPM P0–P2a 工程执行蓝图

日期：2026-09-21  
适用基线：PecoFence `9e5ea0b4f3d71971d5a0cd717037fa6be920398e`，SPM `e37e0ee21ee73784a5d5c7c06e8f0899d117fa18`  
上位约束：`NEXT_PHASE_ROADMAP_AND_PLANNING.md`、`REDESIGN_UIUX_AND_CORDIS_ARCHITECTURE.md`

本文只规划 P0、P1、P2a。P2b 的领域重构、新数据库和真实连接器不进入本批实现。P2a 通过合成读模型提供完整 v2 方法行为，不把 fixture 数据写入 v1 数据库，也不调用 Jira、Meegle 或 Gerrit。

## 1. 实施边界与依赖顺序

实施依赖如下：

```text
SPM P0 contracts ──┬── PecoFence P0 typed consumer ── P1 lifecycle
                  └── SPM P2a transport/listener ──── P3 Windows 联调
```

阶段合并门槛：

| 阶段 | 必须完成 | 不计为完成 |
| --- | --- | --- |
| P0 | 两仓库使用同一 `spm-contracts` 版本；golden 与负面 fixture 通过；PecoFence 不再定义 SPM wire DTO 或拼接 SPM JSON | 只复制相同结构到两个仓库；只修改协议版本常量 |
| P1 | 单一 activation 所有权；open/reconcile/attach/detach/reconfigure/close 全部进入非阻塞 stop/drain；真实 WinEvent wrapper 能在回调内撤销 | 只扩展 `plugin-kernel::panel::PanelManager` 而宿主仍保留第二套生命周期；UI 线程同步等待 |
| P2a | v2 pipe listener、持续 reader、单 writer、多路请求和订阅、九个 RPC、取消及有界队列在 fixture 模式可运行 | 在 v1 enum 上增加 variant；每订阅建立一条 pipe；使用 `select` 取消半帧 reader |

P0 先在 SPM 仓库提交。`spm-contracts` 发布为精确版本 `0.1.0` 后，PecoFence 使用 `=0.1.0`。联合开发者可在未提交的本地 Cargo 配置中把该版本 patch 到 `../spm/crates/spm-contracts`。PecoFence CI 必须在相邻 `spm` 目录不存在时完成解析、构建和测试，因此仓库内不提交相邻路径依赖。

## 2. P0：`spm-contracts` 结构

### 2.1 crate 文件布局

在 SPM 仓库新增：

```text
crates/spm-contracts/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── version.rs
│   ├── ids.rs
│   ├── endpoint.rs
│   ├── envelope.rs
│   ├── rpc.rs
│   ├── error.rs
│   ├── query.rs
│   ├── snapshot.rs
│   ├── framing.rs
│   └── canonical.rs
├── schema/
│   ├── envelope-v2.schema.json
│   ├── project-snapshot-v2.schema.json
│   └── rpc-v2.schema.json
├── fixtures/
│   ├── catalog.json
│   ├── snapshots/
│   ├── pages/
│   ├── rpc/
│   └── invalid/
└── tests/
    ├── golden.rs
    ├── schema_negative.rs
    ├── framing.rs
    └── lifecycle_rules.rs
```

SPM 根 `Cargo.toml` 把 `crates/spm-contracts` 加入 workspace members，并增加 workspace dependency：

```toml
spm-contracts = { path = "crates/spm-contracts", version = "0.1.0" }
```

crate 依赖限定为 `serde`、`serde_json`、`chrono`、`uuid` 和 `thiserror`。该 crate 不依赖 `spm-domain`、`spm-application`、`spm-store-sqlite`、Tokio、HTTP、Win32 或 UI crate。Envelope、请求参数和关键读模型结构使用 `#[serde(deny_unknown_fields)]`；允许 minor 扩展的 response 结构把 optional 扩展放入显式 `extensions` map，旧 consumer 忽略不认识的 extension key。关键状态枚举不使用 catch-all variant，未知值必须产生 UnsupportedFeature。

`lib.rs` 只重导出公共契约。`framing.rs` 提供同步纯函数和常量；异步 I/O 状态机放在 `spm-protocol`，避免 contracts 依赖 runtime。

### 2.2 版本、endpoint 与 frame

`version.rs`：

```rust
pub const PROTOCOL_MAJOR: u16 = 2;
pub const PROTOCOL_MINOR: u16 = 0;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_PAGE_SIZE: u16 = 100;
pub const MAX_PAGE_SIZE: u16 = 500;
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
```

`endpoint.rs` 不调用系统 API，只接受已验证的身份输入：

```rust
pub struct EndpointIdentity {
    pub user_sid: String,
    pub windows_session_id: u32,
}

pub fn v2_pipe_name(identity: &EndpointIdentity) -> Result<String, ContractError>;
```

`user_sid` 必须符合 `S-` 开头、ASCII 数字和连字符格式，长度上限 184；session ID 使用十进制。输出固定为 `\\.\pipe\pecofence.spmd.v2.<user-sid>.<session-id>`。PecoFence 与 spmd 各自在平台层从进程 token 和 `ProcessIdToSessionId` 取得输入，不能读取 `USERNAME`、`USER` 或可变环境变量。

`framing.rs` 提供：

```rust
pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, ContractError>;
pub fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T, ContractError>;
pub fn validate_declared_length(prefix: [u8; 4]) -> Result<usize, ContractError>;
```

长度为 little-endian u32，只接受 `1..=4_194_304`。`decode_frame` 要求输入长度恰好为 `4 + declared_length`。非法 UTF-8、非法 JSON、尾随字节和截断帧均返回错误。异步 reader 必须先调用 `validate_declared_length`，再分配 payload。

### 2.3 标识、Envelope 与消息分类

`ids.rs` 定义透明 UUID newtype，禁止跨用途复用：

```rust
pub struct DaemonSessionId(pub Uuid);
pub struct RequestId(pub Uuid);
pub struct SubscriptionId(pub Uuid);
pub struct OperationId(pub Uuid);
pub struct BriefingId(pub Uuid);
pub struct IdempotencyKey(pub Uuid);
pub struct ProjectId(pub String);
pub struct DeliveryScopeId(pub String);
pub struct BaselineId(pub String);
pub struct RecordId(pub String);
```

字符串业务 ID 在反序列化时执行非空、去首尾空格、最大 256 bytes 校验。UUID 采用带连字符的小写文本。revision 为 `u64` newtype，零允许用于初始 fixture，但生产发布从 1 开始。

`envelope.rs`：

```rust
pub struct Envelope {
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub daemon_session: Option<DaemonSessionId>,
    pub request_id: Option<RequestId>,
    pub subscription_id: Option<SubscriptionId>,
    pub revision: Option<Revision>,
    #[serde(flatten)]
    pub body: Body,
}

#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum Body {
    Request(Request),
    Response(Response),
    Event(Event),
    Error(ErrorDto),
}
```

`Request` 和 `Response` 分别使用 `#[serde(tag = "method", content = "params", rename_all = "snake_case")]`。`Event` 使用 `#[serde(tag = "event", content = "data", rename_all = "snake_case")]`。

字段规则由 `Envelope::validate(Direction)` 集中执行：

| 消息 | daemon_session | request_id | subscription_id | revision |
| --- | --- | --- | --- | --- |
| Hello request | 无 | 必填 | 无 | 无 |
| Hello response/error | 必填；版本不匹配 error 可无 | 与请求相同 | 无 | 无 |
| 其他 request/response/error | 必填 | 必填 | 按方法可选 | 按方法可选 |
| Snapshot event | 必填 | 无 | 必填 | 必填 |
| Operation event | 必填 | 无 | 无 | 无 |

握手后收到 session 不一致的 envelope 时，transport 丢弃消息并记录 stale-session 诊断，不能让 `ProjectSnapshot` 改写当前 session。响应的 `request_id` 必须存在于请求表；未知或已超时 ID 只产生 late-response 诊断。

### 2.4 九个 RPC 的 typed contract

`rpc.rs` 的请求与响应 variant 固定为九项：

```rust
pub enum Request {
    Hello(HelloRequest),
    GetCapabilities(GetCapabilitiesRequest),
    Subscribe(SubscribeRequest),
    Unsubscribe(UnsubscribeRequest),
    QueryPage(QueryPageRequest),
    Refresh(RefreshRequest),
    ResolveNavigation(ResolveNavigationRequest),
    BuildBriefing(BuildBriefingRequest),
    Ping(PingRequest),
}

pub enum Response {
    Hello(HelloResponse),
    GetCapabilities(GetCapabilitiesResponse),
    Subscribe(SubscribeResponse),
    Unsubscribe(UnsubscribeResponse),
    QueryPage(QueryPageResponse),
    Refresh(RefreshResponse),
    ResolveNavigation(ResolveNavigationResponse),
    BuildBriefing(BuildBriefingResponse),
    Ping(PingResponse),
}
```

具体 DTO：

```rust
pub struct HelloRequest {
    pub client_build: String,
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub features: BTreeSet<Feature>,
}
pub struct HelloResponse {
    pub daemon_build: String,
    pub daemon_session: DaemonSessionId,
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub features: BTreeSet<Feature>,
    pub limits: TransportLimits,
}

pub struct GetCapabilitiesRequest;
pub struct GetCapabilitiesResponse {
    pub query_views: BTreeSet<ViewKind>,
    pub methods: BTreeSet<RpcMethod>,
    pub config_entry: Option<ConfigEntry>,
}

pub struct SubscribeRequest { pub query: ProjectQuery }
pub struct SubscribeResponse {
    pub subscription_id: SubscriptionId,
    pub accepted_query: ProjectQuery,
    pub initial_revision: Revision,
}

pub struct UnsubscribeRequest { pub subscription_id: SubscriptionId }
pub struct UnsubscribeResponse { pub removed: bool }

pub struct QueryPageRequest {
    pub subscription_id: SubscriptionId,
    pub revision: Revision,
    pub cursor: Option<PageCursor>,
    pub page_size: u16,
}
pub struct QueryPageResponse {
    pub revision: Revision,
    pub items: Vec<WorkItem>,
    pub next_cursor: Option<PageCursor>,
}

pub struct RefreshRequest {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub idempotency_key: IdempotencyKey,
}
pub struct RefreshResponse {
    pub operation_id: OperationId,
    pub disposition: RefreshDisposition,
}

pub struct ResolveNavigationRequest {
    pub source: SourceRef,
    pub revision: Revision,
}
pub struct ResolveNavigationResponse {
    pub targets: Vec<NavigationTarget>,
}

pub struct BuildBriefingRequest {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub revision: Revision,
    pub locale: String,
    pub timezone: String,
}
pub struct BuildBriefingResponse {
    pub briefing_id: BriefingId,
    pub revision: Revision,
    pub text: String,
    pub html: Option<String>,
    pub evidence_markers: Vec<EvidenceMarker>,
}

pub struct PingRequest { pub correlation_id: Uuid }
pub struct PingResponse {
    pub correlation_id: Uuid,
    pub daemon_session: DaemonSessionId,
}
```

`ProjectQuery` 包含 `project_id`、`delivery_scope_id`、`view`、`filter`、`detail_level` 和稳定 `sort`。相同 query 的等价性使用规范化字段值计算，不使用序列化字节的偶然顺序。`PageCursor` 是 opaque base64url 文本，内部由 daemon 绑定 query hash、filter hash、sort、revision 和最后稳定键。contracts 只校验格式和长度，签名与解码由 daemon 实现。

`Event` 至少包括：

```rust
pub enum Event {
    Snapshot(ProjectSnapshot),
    OperationCompleted(OperationCompleted),
}
```

`OperationCompleted` 包含 `operation_id`、`RefreshOutcome`、开始和结束时间、可选脱敏错误。Refresh response 只表示 accepted 或 coalesced，不表示采集完成。

### 2.5 错误码

`error.rs` 定义稳定枚举：

```rust
pub enum ErrorCode {
    ProtocolMismatch,
    UnsupportedFeature,
    InvalidRequest,
    UnknownScope,
    UnknownSubscription,
    SnapshotExpired,
    Backpressure,
    Unavailable,
    PermissionDenied,
    Cancelled,
    DeadlineExceeded,
    NotFound,
    Conflict,
    Internal,
}

pub struct ErrorDto {
    pub code: ErrorCode,
    pub request_id: Option<RequestId>,
    pub retryable: bool,
    pub message: String,
    pub details: Option<ErrorDetails>,
}
```

`message` 不包含 token、源记录正文、文件路径、SQL 或 stack trace。`retryable` 规则固定：Backpressure、Unavailable、DeadlineExceeded 可为 true；ProtocolMismatch、UnsupportedFeature、InvalidRequest、PermissionDenied 固定为 false；SnapshotExpired 由调用方重新取得 snapshot，不重试旧 cursor。framing 错误不发送 ErrorDto，因为连接边界已经不可信，服务端关闭该连接。

### 2.6 `ProjectSnapshot` typed read model

`snapshot.rs` 不引用 domain entity。最小结构为：

```rust
pub struct ProjectSnapshot {
    pub project_id: ProjectId,
    pub delivery_scope_id: DeliveryScopeId,
    pub baseline_id: Option<BaselineId>,
    pub policy_revision: u64,
    pub daemon_session: DaemonSessionId,
    pub revision: Revision,
    pub computed_at: DateTime<Utc>,
    pub project_timezone: String,
    pub freshness_ttl_secs: u32,
    pub sources: Vec<SourceEvidence>,
    pub overall_gate: OverallGate,
    pub gates: Vec<GateEvaluation>,
    pub next_milestone: Option<MilestoneSummary>,
    pub work_summary: WorkSummary,
    pub preview_items: Vec<WorkItem>,
    pub customer_obligations: CustomerObligationSummary,
    pub verification: VerificationSummary,
    pub merge: MergeSummary,
    pub trend: TrendSummary,
}
```

相关 enum 和结构：

- `SourceSystem { Jira, Meegle, Gerrit }`。
- `CoverageState { Complete, Partial, Unknown }`。`SourceEvidence` 包含 source、tenant、observed_at、source_revision、watermark、coverage、page_complete、permission_complete、freshness_ttl_secs 和可选 `SourceErrorCode`。源失败时保留最近成功 observed_at。
- `GateState { Satisfied, Unsatisfied, Unknown, NotApplicable }`。`OverallGate` 包含 state、unsatisfied_count、unknown_count、applicable_count。
- `GateEvaluation` 包含 gate_id、label、state、applicability、`n`、`v`、`threshold: Option<Ratio { p, q }>`、required、gap、deadline、reason_codes 和 evidence refs。所有计数使用 u64；q 必须大于零；required 与 gap 是 daemon 已计算值，consumer 不重算。
- `WorkItem` 包含稳定 record_id、kind、title、owner、due_at、next_step、source_refs、relation_state。`RelationState { Confirmed, Candidate, Rejected, Unrelated }`。
- `SourceRef` 固定五元组 system、tenant、project、record_kind、record_id。URL 不属于 SourceRef。
- `NavigationTarget` 只包含 system、configured_origin_id 和编码后的相对记录路径；PecoFence NavigationService 根据已配置 origin 生成并复核 HTTPS URL。
- `TrendSummary` 包含 `TrendBasis { CohortFixed, LiveScope }`、有序 points、可选 plan line；`TrendPoint` 的业务计数为 optional，缺失点不能填零。
- milestone、customer obligation、verification 和 merge 使用独立结构，不能从 merged 推导 verified 或 accepted。

所有时间为 RFC 3339 UTC instant；项目时区为 IANA name。展示字符串、相对时间和 gate 颜色不进入 wire model。

### 2.7 合成 fixture 数据集

fixtures 使用固定 daemon session：

- session A：`11111111-1111-4111-8111-111111111111`
- session B：`22222222-2222-4222-8222-222222222222`
- 所有 computed_at：`2026-09-21T01:20:00Z`

`fixtures/catalog.json` 精确列出以下四个 scope：

| 文件 | project/scope | revision | 必须表达的事实 |
| --- | --- | ---: | --- |
| `snapshots/atlas-eu-r42.json` | `project-atlas/scope-eu` | 42 | overall Unsatisfied；一个已知失败和一个 Unknown；N=43、V=38、p=95、q=100、required=41、gap=3；Jira complete；Meegle partial 且 permission incomplete；Gerrit unknown；baseline `atlas-build-2026.09.20.7` |
| `snapshots/atlas-cn-r7.json` | `project-atlas/scope-cn` | 7 | overall Unknown；与 EU 共用 project 但 scope、baseline 和 source watermark 不同；用于证明 scope 不串数据 |
| `snapshots/boreal-in-r3.json` | `project-boreal/scope-in` | 3 | 零分母规则为 NotApplicable；无 baseline、无 next milestone、无 plan line；三源 complete；中英文长标题和 200% 文本数据 |
| `snapshots/cygnus-global-r99.json` | `project-cygnus/scope-global` | 99 | overall Satisfied；三个来源 complete；同名事项使用不同 record_id；一个事项有 Jira、Meegle、Gerrit 三个 SourceRef；导航返回多个候选目标 |

Atlas EU 的 gate 列表固定为：

1. `verified-close-ratio`：Unsatisfied，N=43、V=38、95/100、required=41、gap=3。
2. `customer-acceptance`：Satisfied，N=12、V=12、1/1、required=12、gap=0。
3. `gerrit-coverage`：Unknown，计数字段为空，reason `coverage_unknown`。
4. `legacy-mobile-check`：NotApplicable，reason `scope_not_applicable`。

Atlas EU preview 恰好三项；`pages/atlas-eu-r42-page-1.json` 含 100 项并返回 cursor，page 2 含 37 项且 cursor 为空。第 100 和第 101 项排序键相邻，用于检测分页重复或跳项。revision 43 发布后，用 r42 cursor 查询必须返回 SnapshotExpired。

额外文件：

- `rpc/<method>-request.json` 与 `rpc/<method>-response.json` 为九个方法各一对，共 18 个 golden。
- `rpc/snapshot-event-atlas-eu-r42.json`、`rpc/operation-completed.json`、`rpc/error-backpressure.json`。
- `rpc/late-session-a-r100.json` 使用 session A 和 revision 100；在已协商 session B 的连接上必须被丢弃。
- `rpc/duplicate-response.json` 重复已完成 request ID；只记录诊断，不重复完成 caller。
- `rpc/unsubscribed-snapshot.json` 使用已撤销 subscription ID；不得投递给 panel。
- `invalid/unknown-gate-state.json`、`unknown-coverage-state.json`、`missing-request-id.json`、`session-on-hello-request.json`、`page-size-501.json`、`revision-zero-page.json`、`bad-timezone.json`、`source-ref-missing-tenant.json`。
- `invalid/frame-zero.bin`、`frame-4194305-prefix.bin`、`truncated-frame.bin`、`invalid-utf8.bin` 和 `trailing-byte.bin`。

golden 规范化规则为递归 key 排序、UTF-8、无浮点数、compact JSON、文件末尾一个换行。`canonical.rs` 把 struct 序列化为 `serde_json::Value` 后递归写入 `BTreeMap`。测试同时执行 fixture → typed value → canonical JSON 比较以及 typed value → JSON → typed value 比较。

## 3. PecoFence 消费 `spm-contracts`

### 3.1 Cargo 与模块边界

修改：

- 根 `Cargo.toml` 增加 workspace dependency `spm-contracts = "=0.1.0"`。
- `crates/plugin-spm/Cargo.toml` 增加 `spm-contracts.workspace = true`。
- `crates/app/Cargo.toml` 的 app 不直接解释 SPM payload；P3 transport adapter 需要时通过新的 `crates/spm-transport-adapter` 依赖 contracts。P0 可先在 app 内建立 `app/src/spm_transport/`，P3 前迁出。
- `crates/plugin-api/src/lib.rs` 删除 `LOCAL_IPC_MAJOR`、`MAX_LOCAL_FRAME`、`encode_local_frame` 和 `decode_local_frame`。通用 panel API 不维护 SPM 协议常量。

`crates/plugin-spm/src/lib.rs` 拆成：

```text
provider.rs
instance.rs
view_model.rs
layout.rs
actions.rs
semantics.rs
lib.rs
```

删除本地 `SpmSnapshot`、`WorkItem`、`NavigationTarget` 和基于 `serde_json::Value` 的字段读取。保留插件配置 DTO，但字段改为 `project_id` 和 `delivery_scope_id`，config major 增至 2。v1 config 不在 v2 中隐式读取。

### 3.2 typed IPC 边界

通用 `IpcService` 仍传递不可变 bytes，避免 plugin-api 依赖 SPM。SPM adapter 只在两个集中入口编解码：

```rust
fn encode_request(envelope: &Envelope) -> Result<Arc<[u8]>, Error>;
fn decode_event(bytes: &[u8]) -> Result<Envelope, Error>;
```

两者直接调用 `spm_contracts`。业务代码只匹配 `Body::Event(Event::Snapshot(snapshot))`、typed response 或 `ErrorDto`。禁止 `serde_json::Value`、`json` macro、字符串字段查找和 `format` 生成 wire JSON。CI 脚本 `scripts/check-spm-wire-boundary.sh` 在 `crates/plugin-spm` 和 transport adapter 中拒绝以下模式：`serde_json::Value`、`serde_json::json`、`.get("method")`、`requestId`、`deliveryScope`。

`SnapshotCursor` 改为由握手状态授权：

```rust
pub struct SnapshotCursor {
    accepted_session: DaemonSessionId,
    query: ProjectQuery,
    revision: Option<Revision>,
}

pub fn accept(&mut self, envelope: &Envelope) -> Result<Accept, SnapshotReject>;
```

只有 session、subscription、project、scope 和 query 均匹配，且 revision 单调增加时才接受。snapshot 内的 daemon_session 必须等于 envelope session。

Refresh、ResolveNavigation 和 BuildBriefing 每次生成独立 request ID。插件保存 pending action map，并通过 `PanelEvent::Completion` 收到 typed response；subscription token 不再复用为 request ID。复制简报必须先接收并展示 `BuildBriefingResponse` 的固定 revision，再由用户动作写剪贴板。

### 3.3 P0 测试

PecoFence 新增：

- `crates/plugin-spm/tests/contracts_golden.rs`：加载发布 crate 的 fixtures，构建 view model，验证四态 gate、partial coverage、零分母、长文本和同名不同 ID。
- `crates/plugin-spm/tests/session_revision.rs`：旧 session、高 revision 旧 session、重复 revision、错误 project/scope、unsubscribe 后事件均拒绝。
- `crates/plugin-api` 的 framing 测试移除；相应测试归 `spm-contracts`。
- `scripts/check-spm-contract-dependency.sh`：在临时目录复制 PecoFence checkout，不存在 `../spm`，运行 `cargo metadata --locked` 和四个 plugin crate 测试。

## 4. P1：Kernel 与 Host 生命周期

### 4.1 单一所有权划分

`crates/plugin-kernel/src/panel.rs` 成为 activation 的唯一所有者。它持有 provider registry、activation records、InstanceScope、MountScope、panel instance 和 drain 状态。`crates/app/src/app/panel_manager.rs` 只保留组合根、窗口到 mount 的绑定、service adapters 和 UI wake 调度，不再保存第二份 `Box<dyn PanelInstance>` 或独立 activation counter。

目标类型：

```rust
pub struct PanelManager {
    providers: HashMap<ProviderId, Rc<dyn PanelProvider>>,
    activations: HashMap<InstanceId, ActivationRecord>,
    retired: HashMap<InstanceKey, RetiredActivation>,
    scopes: ScopeTree,
    supervisor: TaskSupervisor,
    services: ServiceRegistry,
    next_activation: u64,
}

pub struct ActivationIdentity {
    pub provider_id: String,
    pub config_major: u16,
    pub config_digest: [u8; 32],
}

pub enum ActivationPhase {
    WaitingDependencies,
    Starting,
    Active,
    Stopping,
    Draining,
    Quarantined,
}
```

同 instance ID 只有一个 current activation。旧 activation 进入 retired 表并完成 drain 后才能启动同 ID 的替代 activation。Quarantined 仍持有 instance、backend、native owner 和 scope，迟到完成由每次 host tick 的 `poll` 回收，不依赖再次 resolve。

`register_provider` 先用 `contains_key` 检查，再 insert。descriptor 的 `api_major`、`config_major` 和 required services 在 create 前校验。config identity 包含 config major 和规范化配置 bytes 的 SHA-256；主题 revision、DPI、device epoch 和数据 revision不进入该 identity。

### 4.2 ActivationTransaction

新增 `crates/plugin-kernel/src/activation.rs`：

```rust
struct ActivationTransaction<'a> {
    manager: &'a mut PanelManager,
    key: InstanceKey,
    provider_scope: ScopeId,
    instance_scope: ScopeId,
    panel: Option<Box<dyn PanelInstance>>,
    initial_mount: Option<MountRecord>,
    committed: bool,
}

impl ActivationTransaction<'_> {
    fn begin(manager: &mut PanelManager, desired: &DesiredPanel) -> Result<Self>;
    fn resolve_context(&mut self, desired: &DesiredPanel) -> Result<PluginContext>;
    fn create(&mut self, desired: &DesiredPanel) -> Result<()>;
    fn attach_initial(&mut self, target: Option<MountRequest>) -> Result<()>;
    fn commit(mut self, identity: ActivationIdentity) -> Result<InstanceKey>;
    fn rollback(&mut self, cause: ActivationFailure);
}
```

顺序固定为：

1. 校验 provider descriptor、config major、config bytes 和 required services。
2. 使用 provider 注册时已创建的 ProviderScope，分配 checked activation ID 和新的 InstanceScope。每个 provider 只有一个 ProviderScope，不为每次 activation 留下空 provider 节点。
3. 解析 required capability；optional capability 缺失不失败，但记录当前 availability。
4. 调用 provider.create。create 成功后立即把 instance 放入 transaction owner，不能只放在栈上后直接返回错误。
5. 如果有初始窗口绑定，创建 MountScope 和 checked mount generation，再调用 mount。
6. 将完整 record 一次性插入 activations 并把 phase 设为 Active。

任何一步失败都调用同一个 rollback：先关闭 transaction 创建的全部 route 与 scope，若 panel 已存在则调用一次 begin_stop，若 mount 已成功则在 callback 退出后调用一次 unmount，然后取消 task/native operation，保留 owner 直到 ledger 为空，最后在所属线程执行 destroy 并 drop panel。rollback 本身只启动状态机，不在 UI turn 等待。事务返回 `ActivationOutcome::PendingRollback` 时，错误视图可以显示失败原因；同 ID 只有在 rollback 完成后才能重试。

### 4.3 对外 API

`plugin-kernel::PanelManager` 公共方法定为：

```rust
pub fn register_provider(&mut self, provider: Rc<dyn PanelProvider>) -> Result<()>;
pub fn reconcile(&mut self, desired: &[DesiredPanel]) -> ReconcileReport;
pub fn open(&mut self, desired: DesiredPanel, initial: Option<MountRequest>)
    -> Result<OpenResult>;
pub fn reconfigure(&mut self, id: InstanceId, replacement: DesiredPanel,
    initial: Option<MountRequest>) -> Result<ReconfigureResult>;
pub fn attach(&mut self, id: InstanceId, request: MountRequest) -> Result<MountKey>;
pub fn detach(&mut self, key: MountKey, reason: DetachReason) -> Result<DetachResult>;
pub fn close(&mut self, id: InstanceId, reason: StopReason) -> Result<CloseResult>;
pub fn poll(&mut self, now: Instant, budget: PollBudget) -> PollReport;
pub fn begin_shutdown(&mut self) -> ShutdownTicket;
pub fn is_shutdown_complete(&self, ticket: ShutdownTicket) -> bool;
```

输入类型：

```rust
pub struct DesiredPanel {
    pub instance_id: InstanceId,
    pub provider_id: String,
    pub config: PanelConfig,
}
pub struct MountRequest {
    pub host: HostSurfaceId,
    pub viewport: RectDip,
    pub dpi: u32,
    pub presentation: Presentation,
}
```

行为规则：

- `reconcile` 对相同 identity 保留 activation；缺失项 close；identity 改变触发 reconfigure；结果按稳定 instance ID 排序。
- `open` 为幂等操作。相同 identity 的 Active 返回 Existing；Stopping/Draining 返回 Pending；不同 identity 要求走 reconfigure。
- `reconfigure` 先停止旧 activation。旧 activation Disposed 或明确 Quarantined 之前不创建 replacement；replacement 作为 manager 内 pending desired 保存，由 `poll` 后续启动。
- `attach` 每次创建新的 MountScope 和单调 generation。一个 instance 可以转移窗口，但首版同一时刻最多一个正文 mount。已有 mount 时返回 Conflict，不隐式 detach。
- `detach` 先关闭 mount route、input 和 gesture 子 scope，等待当前 callback depth 为零，再调用一次 `panel.unmount(key)`，随后排空 mount owned resources。instance 和摘要 SubscriptionScope 保留。
- `close` 先 detach 全部 mounts，再调用一次 begin_stop，停止 SubscriptionScope 和 InstanceScope。重复 close 返回 AlreadyStopping。
- `begin_shutdown` 只启动所有 root child 的停止。app 继续 pump Windows messages 并调用 poll；没有逐实例五秒同步等待。
- 五秒是诊断阈值。超过阈值进入 Quarantined 并报告 pending ledger IDs；不 drop owner，不标记 Disposed。

宿主调用映射：关闭标签→close；删除 container→逐 panel close；切换标签→旧 detach、新 attach；拖出/合并→旧 detach 完成后 attach 同一 instance；Capsule→正文 detach；展开→attach；退出→begin_shutdown。

### 4.4 scope、ledger 与 completion

修改 `scope.rs`：

- 增加 `Subscription`、`Window`、`Shell`、`Peek` scope kind。
- 将当前 `Plugin` scope kind 重命名为 `Provider`；provider 注册时创建一个 ProviderScope，注销 provider 时在其全部 activation 排空后回收。
- `finish_dispose` 改为 kernel 私有，并在调用前校验 tasks、native_ops、callbacks、cleanup 和 children 五个 ID 集合为空。
- 增加 `remove_disposed_subtree(root)`，从父 children 和 arena 删除 record；root 与 ServiceScope 不允许删除。
- effect 使用 `EffectId`，支持提前 revoke 和 tombstone 压缩。

修改 `supervisor.rs`：

- token 改为 `{ owner, activation, service_generation, sequence }`，外部不能构造。
- task completion 从 JoinHandle 实际结果采集 Normal、Cancelled、Panicked，不依赖 task 尾部自行发送 completion。
- native record 保存 buffer、OVERLAPPED owner、cancel handle 和 completion generation，直到完成包被观察。
- 增加 callback guard 与 cleanup guard；Drop 时按 ID 完成一次，重复 completion 返回 DuplicateCompletion 诊断。
- `poll` 有事件数和耗时预算，不 sleep、不 join 未完成任务、不 block_on。

新增 `drain.rs` 的核心接口：

```rust
pub fn begin_stop_tree(&mut self, root: ScopeId, reason: StopReason) -> Result<()>;
pub fn enter_callback(&mut self, owner: ScopeId) -> Result<CallbackGuard>;
pub fn begin_cleanup(&mut self, owner: ScopeId) -> Result<CleanupGuard>;
pub fn record_native_completion(&mut self, token: NativeToken, generation: u64)
    -> CompletionDisposition;
pub fn poll_drain(&mut self, root: ScopeId, budget: PollBudget) -> DrainState;
```

停止顺序为整棵子树先封闭入口、撤销 route/lease/input、发取消、等待当前 callback 退出、unmount、观察所有 ledger、所属线程 destroy、drop instance、回收 arena。

### 4.5 WinEvent wrapper 生命周期

当前 `winevent.rs` 在 callback 前从 map 取出 callback，callback 内 Drop 时 map 已无 entry，返回后会再次插入。修改为显式 registration state：

```rust
struct HookState {
    raw: HWINEVENTHOOK,
    generation: u64,
    alive: Cell<bool>,
    callbacks_in_flight: Cell<u32>,
}

struct CallbackEntry {
    state: Rc<HookState>,
    callback: Option<EventCallback>,
}

pub struct WinEventRegistration {
    state: Rc<HookState>,
}
```

Drop 顺序为 `alive=false`、从 CALLBACKS 删除同 raw 且同 generation 的 entry、调用 UnhookWinEvent。`event_proc` 取 callback 时保留 `Rc<HookState>`，先确认 alive 和 generation，增加 callbacks_in_flight，调用后用 guard 减少计数；仅当 alive 仍为 true、generation 相同且 map 中仍保留同一 registration slot 时归还 callback。回调内 Drop 后 alive 为 false，因此不能复活。

平台层新增：

```rust
pub fn install_scoped(
    owner: &ScopeHandle,
    event_min: u32,
    event_max: u32,
    callback: EventCallback,
) -> Result<WinEventRegistration>;
```

kernel adapter 在安装后立即把 registration 登记到 owner effect，并把 callback guard 接到 drain ledger。WinEvent 对象只能在安装它的 message-loop 线程 unhook；跨线程 stop 把 cleanup 投递到该 dispatcher，并把 CleanupId 保留到执行完成。

### 4.6 P1 测试

新增或扩展以下测试：

1. provider duplicate 不替换原 provider。
2. config bytes 相同但 config major 不同会 reconfigure。
3. required service 第 N 次 resolve 失败、create 失败、mount 失败均不留下 open route，且已创建资源按逆序释放。
4. attach generation 单调；旧 generation 的 input、snapshot 和 completion 被拒绝。
5. detach 等 callback 退出后只调用一次 unmount。
6. close 后无需再次 resolve，poll 能把 activation 回收到 arena 基线。
7. reconfigure 在旧 activation drain 前不启动新 activation。
8. task 正常、取消和 panic 都能完成 ledger；release 的 panic abort 行为只做进程级测试，不宣称进程内恢复。
9. 跨 owner cancel 被拒绝；重复 native completion 不减少其他记录。
10. 五秒后 Quarantined 仍保留 owner；迟到 completion 后回收。
11. fake adapters 连续 1000 次 open/attach/detach/close，scopes、effects、consumers、tasks 和 native records 回到初始计数。
12. WinEvent callback 内 drop registration，第二次 synthetic event 不执行 callback，map 无残留。
13. WinEvent stop 与 callback 重入交错，generation 不复活，cleanup 在原线程完成。

## 5. P2a：SPM v2 IPC transport 与 listener

### 5.1 文件拆分

保留 v1 且不与 v2 type alias：

```text
crates/spm-protocol/src/
├── ipc/
│   ├── mod.rs
│   ├── v1.rs              # 现有 ipc.rs 移入，行为不变
│   └── v2/
│       ├── mod.rs
│       ├── frame_reader.rs
│       ├── frame_writer.rs
│       ├── connection.rs
│       ├── router.rs
│       ├── quotas.rs
│       └── memory_stream_tests.rs
└── lib.rs

crates/spmd/src/
├── main.rs
├── args.rs
├── shutdown.rs
├── v1_host.rs
└── v2/
    ├── mod.rs
    ├── listener.rs
    ├── security.rs
    ├── session.rs
    ├── fixture_service.rs
    └── router_service.rs
```

`spm-protocol` 和 `spmd` 增加 `spm-contracts.workspace = true`。`spmd` CLI 增加：

```text
--ipc-version v1|v2
--fixture <path-to-catalog>
--fixture-db <path>
--max-connections <n>      默认 32
```

`--fixture` 与真实 live config 互斥。v2 fixture 模式不要求 `--config`，只加载 contracts fixtures。fixture DB 路径必须位于 `spmd-v2-fixture` 命名空间，拒绝与 v1 `SPM_DB` 同路径。

### 5.2 Windows endpoint 与 listener

`security.rs` 从当前进程 token取得 user SID，从当前进程取得 Windows SessionId，再调用 contracts resolver。DACL 至少允许当前用户和 SYSTEM，拒绝 remote clients，第一条 pipe 使用 first-pipe-instance。listener 启动时生成 UUID `daemon_session`，它与 Windows SessionId 无关。

```rust
pub struct ListenerConfig {
    pub endpoint: String,
    pub max_connections: usize,
    pub shutdown: CancellationToken,
}

pub async fn serve_v2(
    config: ListenerConfig,
    session: Arc<DaemonSession>,
    service: Arc<dyn ReadModelService>,
) -> io::Result<()>;
```

accept loop 规则：

1. 启动前创建 first instance；失败即退出，不能降级为 v1。
2. 当前 server connect 成功后，在 spawn session 前先创建下一 server instance，使 endpoint 保持可接受连接。
3. 达到连接上限时完成握手前直接关闭新连接并记录 connection_limit；不进入无界 JoinSet。
4. 每个 session 放入 JoinSet；主循环持续收集结束结果。
5. shutdown 时停止创建和接受新 pipe，取消全部 session，关闭 router 输入，等待 reader、writer、fixture producer 和 session join。诊断期限超出时记录 pending task，不把 abort 当作已排空证据。

### 5.3 持续 reader、单 writer 与 connection actor

每条 pipe 固定三个任务：

```text
FrameReader ── bounded inbound ── ConnectionActor ── bounded outbound ── FrameWriter
                                      │
                                      └── ReadModelService
```

`FrameReader` 独占 `ReadHalf`，从连接建立到 EOF 或取消持续运行。它维护 `ReadingPrefix { filled }` 和 `ReadingPayload { declared, filled, buffer }` 状态，不被命令、heartbeat 或 snapshot 更新所取消。只允许取消整条连接；取消后丢弃 stream，不复用半帧。

```rust
pub async fn run_reader<R: AsyncRead + Unpin>(
    reader: R,
    inbound: mpsc::Sender<Envelope>,
    cancel: CancellationToken,
    quota: Arc<ConnectionQuota>,
) -> ReaderExit;
```

prefix 完整后先校验长度，再向 quota 申请 bytes，再分配 payload。解析和 `Envelope::validate` 成功后才入队。inbound 容量 64；满时 reader 等待会产生 pipe backpressure。控制队列不能静默丢弃。

`FrameWriter` 独占 `WriteHalf`，只有它调用 write。它从两个来源调度：控制响应 FIFO 容量 128；每 subscription 一个容量 1 的 latest snapshot slot。每次先发送有限数量控制响应，再轮询 snapshot slot，避免任一来源饥饿。完整 frame 的 `write_all` 或 flush 被取消、超时或部分失败后关闭连接，不重用 writer。

```rust
pub enum Outbound {
    Control(Envelope),
    Snapshot { subscription: SubscriptionId, envelope: Arc<Envelope> },
}
```

connection 默认总内存 quota 32 MiB，包含 reader buffer、encoded writer frame、inbound/outbound payload 和 retained page data。单 frame 仍受 4 MiB 限制。配额不足时：请求产生 Backpressure error；snapshot 覆盖该 subscription 的旧 latest value；控制 response 无空间时暂停读取直到释放，超过 request deadline 后关闭连接并记录 backpressure_timeout。

### 5.4 握手、multiplex 和请求表

`ConnectionActor` 状态：

```rust
pub enum ConnectionPhase { AwaitHello, Active, Closing }

pub struct ConnectionState {
    pub daemon_session: DaemonSessionId,
    pub negotiated_minor: u16,
    pub features: BTreeSet<Feature>,
    pub subscriptions: HashMap<SubscriptionId, SubscriptionState>,
    pub requests: HashMap<RequestId, RequestState>,
    pub last_valid_message: Instant,
}
```

Hello 必须在 5 秒内成为首个有效消息。major 不同返回 ProtocolMismatch 后关闭。minor 取双方较小值，feature 为交集。Active 状态拒绝第二个 Hello。所有其他 request 必须携带已协商 daemon_session。

九个方法路由到：

```rust
pub type ServiceFuture<'a, T> = Pin<
    Box<dyn Future<Output = Result<T, ServiceError>> + Send + 'a>
>;

pub trait ReadModelService: Send + Sync {
    fn capabilities(&self) -> ServiceFuture<'_, Capabilities>;
    fn subscribe(&self, query: ProjectQuery) -> ServiceFuture<'_, SubscriptionSource>;
    fn query_page(&self, query: PageQuery) -> ServiceFuture<'_, Page<WorkItem>>;
    fn refresh(&self, command: RefreshCommand) -> ServiceFuture<'_, RefreshReceipt>;
    fn resolve_navigation(&self, command: NavigationCommand)
        -> ServiceFuture<'_, Vec<NavigationTarget>>;
    fn build_briefing(&self, command: BriefingCommand)
        -> ServiceFuture<'_, Briefing>;
}
```

Hello、Unsubscribe 和 Ping 由 actor 本地处理；其他六项调用 service。每个 request 建立独立 cancellation token 和 10 秒 deadline。相同 connection 上可并发处理不同 request，response 按 request ID 关联，不要求按发起顺序返回。

Subscribe 分配随机 subscription ID。`SubscriptionSource` 提供 initial snapshot 和 watch receiver。actor 在同一连接内允许多个项目和 scope；相同规范化 query 在 daemon `SubscriptionHub` 中引用计数复用 producer，最后一个跨连接 consumer 退出时释放。Unsubscribe 先从 route map 删除，再取消 forwarder；之后到达的 snapshot 因 subscription 不存在被丢弃。

Refresh 幂等表 key 为 `(daemon_session, project_id, delivery_scope_id, idempotency_key)`。相同 scope 已有采集时返回 Coalesced 和同一 operation ID。fixture service 用受控 timer 发送 OperationCompleted，并只在数据实际改变时发布新 revision。

Ping 返回 correlation ID 和当前 daemon session，不改变 snapshot revision、observed_at 或 freshness。

### 5.5 fixture service 行为

`fixture_service.rs` 启动时验证全部 fixture，再构建不可变 revision store：

```rust
pub struct FixtureReadModelService {
    catalog: FixtureCatalog,
    revisions: RwLock<HashMap<QueryKey, BTreeMap<Revision, Arc<ProjectSnapshot>>>>,
    hubs: Mutex<HashMap<QueryKey, WatchHub>>,
    operations: Mutex<HashMap<IdempotencyKey, OperationRecord>>,
}
```

默认 subscribe 返回 catalog 指定 revision。QueryPage 只从请求 revision 的 immutable page set 读取。测试控制接口只在 fixture build feature 下可用，可将 Atlas EU 从 r42 推进到 r43、模拟 source unavailable、完成 refresh 或重启 daemon session；正式 build 不暴露该接口。

ResolveNavigation 依据 SourceRef 和 fixture 中配置的 origin map 生成结构化 target。fixture 中存的是 origin ID 和 record ID，不读取 source payload URL。BuildBriefing 只接受仍保留的 revision，输出固定 UTF-8 text、可选 HTML 和 evidence markers。

### 5.6 P2a transport 测试

使用 `tokio::io::duplex` 和可控 chunk reader，覆盖：

1. prefix 按 1/2/1 bytes 分片、payload 每 byte 分片、两帧粘连和连续 100 帧。
2. declared length 为 0、4 MiB、4 MiB+1；超限在 payload allocation 前返回。
3. prefix EOF、payload EOF、非法 UTF-8、非法 JSON、尾随 JSON。
4. command 到达时 reader 正处于半帧，处理 command 不丢失半帧状态。
5. 取消 reader 后连接被弃用；writer 部分写失败后不发送第二帧。
6. Hello 5 秒 timeout、major mismatch、minor feature negotiation、第二次 Hello。
7. 九个 RPC 的 request/response request ID 关联；response 乱序仍对应正确 caller。
8. 一个连接同时订阅 Atlas EU、Atlas CN 和 Boreal IN，更新互不串 route。
9. 两个 consumer 订阅相同 query，hub refcount 为 2；逐个 unsubscribe 后为 1 和 0。
10. unsubscribe 后迟到 snapshot、未知 subscription 和 daemon session 变化。
11. control queue 128 边界、snapshot latest slot 覆盖、32 MiB byte quota、慢 consumer。
12. r42 page 1 后发布 r43，r42 page 2仍从 retained r42 返回；回收 r42 后返回 SnapshotExpired。
13. Refresh 同 idempotency key 返回同 operation；不同 key 但已有运行任务返回 Coalesced；完成事件只发一次。
14. Ping 不推进 revision 或 observed_at。
15. shutdown 停止 accept，reader/writer/producer/session JoinSet 均被观察完成。

Windows 集成测试在 P3 执行，但 P2a 增加 `#[cfg(windows)]` smoke test：endpoint 同 SID 不同 SessionId 不同；first instance 阻止第二 server；remote clients 标志和 DACL 创建成功。完整的跨用户拒绝和客户端 server PID/身份校验属于 P3 实机矩阵。

## 6. 逐提交实施序列

每个编号形成一个可独立审核的提交；后一项不在前一项测试失败时开始。

1. SPM 新建 `spm-contracts` crate、ID、version、endpoint 和 framing；加入 workspace；完成 frame 与 endpoint 单元测试。
2. 增加 Envelope、九个 RPC、ErrorDto、ProjectSnapshot 和 validate；生成三项目四 scope fixtures、18 个 RPC golden 及负面 fixtures。
3. 增加 canonical、schema 和 golden tests；发布精确版本 0.1.0；记录制品 checksum。
4. PecoFence 引入 `spm-contracts = "=0.1.0"`；删除 plugin-api 的 SPM framing；plugin-spm 删除私有 snapshot DTO 和手写 JSON；增加 wire boundary 检查脚本。
5. PecoFence 把 plugin-spm config major 升为 2，接入 typed query、session/revision 校验和 pending request completion；运行四个 plugin crate 测试及 Windows cargo check。
6. 修复 kernel 与 host 两处 provider duplicate 行为；增加 activation identity 和 descriptor/config/service 校验。
7. 新建 `activation.rs` 与 `drain.rs`；把 create 和 initial mount 纳入 transaction；注入每一步失败测试。
8. 实现 reconcile/open/reconfigure/attach/detach/close/poll；将标签、container、Capsule、拖出/合并和 shutdown 调用点逐一替换；删除宿主第二份实例所有权。
9. 扩展 ScopeTree、TaskSupervisor、service replacement barrier 和 arena 回收；完成 1000 次 fake lifecycle 压力测试。
10. 修改真实 `platform/winevent.rs`，接入 alive/generation/callback guard 和 dispatcher cleanup；执行重入及回调内撤销测试。
11. SPM 将现有 `ipc.rs` 移为 `ipc/v1.rs`，保持 v1 tests；建立 v2 frame reader/writer 和 quota tests。
12. 实现 v2 ConnectionActor、握手、请求表、单连接多订阅和九个方法 router；完成 memory duplex 套件。
13. 实现 FixtureReadModelService、revision store、pagination、refresh operation、navigation 和 briefing；加载 P0 fixtures。
14. 实现 Windows v2 listener、endpoint、DACL、accept loop 和 structured shutdown；增加 fixture CLI 参数和 Windows smoke tests。
15. 两仓库运行完整测试、clippy、format、Windows target check；PecoFence 在无相邻 SPM checkout 环境运行依赖检查；保存测试输出作为 P0/P1/P2a 阶段证据。

## 7. 文件级修改清单

SPM：

- 修改 `/mnt/c/Users/Administrator/spm/Cargo.toml`。
- 新建 `/mnt/c/Users/Administrator/spm/crates/spm-contracts/**`。
- 修改 `/mnt/c/Users/Administrator/spm/crates/spm-protocol/Cargo.toml` 和 `src/lib.rs`。
- 移动 `spm-protocol/src/ipc.rs` 到 `src/ipc/v1.rs`，新建 `src/ipc/v2/**`。
- 修改 `/mnt/c/Users/Administrator/spm/crates/spmd/Cargo.toml`，拆分 `src/main.rs` 并新建 `src/v2/**`。
- v1 `spm-protocol/src/odm.rs` 保留给旧客户端，不 type alias 到 ProjectSnapshot。

PecoFence：

- 修改根 `Cargo.toml`、`Cargo.lock`、`crates/plugin-spm/Cargo.toml`、`crates/app/Cargo.toml`。
- 修改 `crates/plugin-api/src/lib.rs`，移除业务协议 framing，补 Completion 和连接事件所需通用类型。
- 重构 `crates/plugin-spm/src/lib.rs` 为六个模块并消费 contracts。
- 重写 `crates/plugin-kernel/src/panel.rs`，新增 `activation.rs`、`drain.rs`，修改 `scope.rs`、`supervisor.rs`、`service.rs`、`lib.rs`。
- 修改 `crates/app/src/app/panel_manager.rs` 和所有 resolve/shutdown/标签/窗口调用点，使宿主只保存 mount binding。
- 修改 `crates/platform/src/named_pipe.rs`，返回 SID 和 Windows SessionId 输入，不在平台层定义错误 endpoint 模板。
- 修改 `crates/platform/src/winevent.rs`，实现 registration state 和 callback lifetime tracking。
- 新增 `scripts/check-spm-wire-boundary.sh` 和 `scripts/check-spm-contract-dependency.sh`。

## 8. 阶段验收命令

SPM：

```text
cargo test -p spm-contracts
cargo test -p spm-protocol
cargo test -p spmd
cargo test --workspace
./scripts/check-windows.sh
```

PecoFence：

```text
cargo test -p pecofence-plugin-api -p pecofence-plugin-kernel -p pecofence-plugin-spm
cargo check --target x86_64-pc-windows-msvc
./scripts/check-spm-wire-boundary.sh
./scripts/check-spm-contract-dependency.sh
```

P1 的 app binary 当前 `test = false`。因此生命周期的可重复行为放入 plugin-kernel fake-host tests，Windows 原生 wrapper 使用独立 test executable。P3 前必须补 host integration harness；仅有 `cargo check` 不作为真实 named pipe、DACL、WinEvent 或窗口生命周期通过证据。

## 9. 停止条件

出现以下任一情况时停止进入下一阶段并保留当前失败证据：

- contracts 需要依赖 domain、store、HTTP 或 Windows 才能表达 DTO。
- 两仓库生成的 canonical golden 不同。
- PecoFence 仍存在 SPM wire `serde_json::Value` 字段读取或字符串 JSON 拼接。
- create/mount 失败后 scope、task、native owner 或 panel instance 无法由 poll 回收。
- WinEvent registration 可在回调返回后复活。
- v2 reader 会因 command、heartbeat 或 snapshot select 分支而丢弃半帧状态。
- 控制响应会静默丢弃，或队列只有条数限制而没有 byte quota。
- fixture daemon 会访问真实连接器、旧 v1 DB 或旧 host 配置。
- daemon shutdown 只调用 abort 而没有观察任务结束。

满足 P0、P1、P2a 各自门槛后，下一阶段 P3 才在 Windows 上接入 PecoFence 共享 transport，并验证 server PID、用户、SessionId、安装身份、双项目 multiplex、断线重连和独立卸载。
