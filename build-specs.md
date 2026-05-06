# forest-guardian — Complete Build Specification

> **Single-source-of-truth build document.**
> Anyone reading this top-to-bottom should be able to implement the entire system without consulting other sources.
>
> Stack: Rust (workspace, multi-crate) + LFM2/2.5-VL via llama-server + SimSat + ONNX Runtime + SQLite + axum/htmx.
>
> Status of this document: authoritative. If code disagrees with this document, the document is right and the code is wrong (or this document needs an ADR amendment).

---

## Table of Contents

1. [Project Mission and Non-Goals](#1-project-mission-and-non-goals)
2. [Ubiquitous Language (Glossary)](#2-ubiquitous-language-glossary)
3. [Architectural Decision Records (ADRs)](#3-architectural-decision-records-adrs)
4. [Domain-Driven Design](#4-domain-driven-design)
5. [System Architecture](#5-system-architecture)
6. [Rust Workspace Layout](#6-rust-workspace-layout)
7. [Crate Specifications](#7-crate-specifications)
8. [Type System (Complete)](#8-type-system-complete)
9. [Database Schema (SQLite DDL)](#9-database-schema-sqlite-ddl)
10. [JSON Schemas](#10-json-schemas)
11. [HTTP API Contract](#11-http-api-contract)
12. [SimSat Client Specification](#12-simsat-client-specification)
13. [Spectral Indices and Masks](#13-spectral-indices-and-masks)
14. [Change Detection and Patch Extraction](#14-change-detection-and-patch-extraction)
15. [Deterministic Scoring](#15-deterministic-scoring)
16. [Legal Context Engine](#16-legal-context-engine)
17. [Foundation Model Embeddings](#17-foundation-model-embeddings)
18. [The Seven VLM Roles](#18-the-seven-vlm-roles)
19. [VLM Backend (llama-server) Integration](#19-vlm-backend-llama-server-integration)
20. [LightGBM Classifier](#20-lightgbm-classifier)
21. [Evidence Packet Specification](#21-evidence-packet-specification)
22. [Audit Log and Provenance](#22-audit-log-and-provenance)
23. [Dashboard Specification](#23-dashboard-specification)
24. [Configuration](#24-configuration)
25. [Fine-Tuning Plan](#25-fine-tuning-plan)
26. [Testing Strategy](#26-testing-strategy)
27. [Success Criteria](#27-success-criteria)
28. [Build Order and Milestones](#28-build-order-and-milestones)
29. [Operational Considerations](#29-operational-considerations)
30. [Real-Data Port Plan](#30-real-data-port-plan)
31. [Appendix A — Reference Index Formulas](#appendix-a--reference-index-formulas)
32. [Appendix B — Synthetic Test Scenarios](#appendix-b--synthetic-test-scenarios)
33. [Appendix C — Crate Dependency Matrix](#appendix-c--crate-dependency-matrix)

---

## 1. Project Mission and Non-Goals

### 1.1 Mission

Detect, score, explain, and package suspected forest disturbance events from SimSat Sentinel-2-like imagery using deterministic multispectral feature extraction, foundation-model embeddings, a small specialist classifier, and a multi-role LFM2/2.5-VL model that performs structured visual reasoning over deterministic outputs. Produce reviewable evidence packets for human enforcement triage.

### 1.2 Operational Output

> "High-confidence suspected forest disturbance inside protected AOI; evidence packet ready for enforcement review."

### 1.3 Non-Goals

The system **does not**:

1. Make legal accusations against named persons or entities.
2. Output "illegal" as a finding — only `possible_unauthorized_activity` based on overlay context, requiring authority review.
3. Replace established detection systems (GLAD-S2, RADD, DIST-ALERT, JRC TMF, JJ-FAST, DETER). It **complements** them by adding the legal-overlay + evidence-packet workflow on top.
4. Claim production-grade precision/recall on synthetic SimSat data. Validation happens after porting to real Sentinel-2.
5. Replace ground verification, permit-record lookup, or judicial process.

### 1.4 What This System Is For

- A demonstration that the architecture works end-to-end on simulated data.
- A reference implementation of the "deterministic core + foundation-model embeddings + small classifier + multi-role VLM" pattern.
- A foundation that ports cleanly to real Sentinel-2 STAC sources.
- A specification of how a compact VLM (LFM2/2.5-VL) can earn its place in a remote-sensing pipeline as the structured-reasoning connective tissue rather than the primary detector.

---

## 2. Ubiquitous Language (Glossary)

These terms are used consistently across code, types, database, and documentation. **Do not introduce synonyms.**

| Term | Definition |
|---|---|
| **AOI** (Area of Interest) | A named geographic region under monitoring, with a polygon and metadata (jurisdiction, type, priority). |
| **Tile** | A fixed-size monitoring patch within an AOI. Default 5 km × 5 km. Has stable ID. |
| **Observation** | One fetched satellite scene for one Tile at one timestamp. Has a unique content hash. |
| **BandStack** | A multiband numerical array for one observation, with named axes (band → 2D array). |
| **FeatureMap** | A single-channel raster derived from a BandStack via a named formula (NDVI, NDMI, NBR, BSI, NDWI). |
| **DeltaMap** | A FeatureMap representing the difference between two FeatureMaps of the same type at different times. |
| **CandidateMask** | A boolean raster where `true` means "this pixel is a candidate for forest disturbance." |
| **Patch** | A connected component within a CandidateMask, with shape statistics. |
| **ChangeEvent** | A set of Patches detected from comparing two observations of the same Tile. |
| **Alert** | An aggregate combining a ChangeEvent, scoring, classifier output, VLM reports, legal context, and review status. |
| **AlertLevel** | One of: `none`, `watch`, `investigate`, `enforcement_review`. |
| **DisturbanceType** | One of an enumerated set: `clear_cut`, `selective_logging`, `logging_road`, `burn_scar`, `flood_or_water_change`, `agriculture_or_harvest`, `cloud_shadow_artifact`, `mining`, `windthrow`, `unknown_disturbance`, `none`. |
| **LegalContext** | The result of overlaying a Patch geometry against legal/protected/permit layers. |
| **EvidencePacket** | A directory of files (composite images, JSON, metadata, hashes) that constitutes the complete record of an Alert. |
| **VlmReport** | A schema-conformant JSON output from a single VLM role invocation. |
| **VlmRole** | One of seven enumerated tasks the VLM performs: triage, classification confirmation, narrative, map reading, reviewer chat, ground-photo cross-check, daily summary. |
| **Score** | A real number in [0, 1]. Distinct named scores: `loss_score`, `quality_score`, `disturbance_score`, `vlm_concurrence`, `embedding_anomaly_score`. |
| **Provenance** | The complete record of inputs, code version, parameters, and timestamps that produced an Alert. Required for every Alert. |
| **Run** | A single execution of the pipeline. Has a unique ID and is recorded in the audit log. |
| **Reviewer** | A human (or designated role) who triages alerts in the dashboard. |
| **AuthorityCase** | A reference to an external enforcement case file. Forest-guardian generates the input; it never closes the loop alone. |

---

## 3. Architectural Decision Records (ADRs)

Each ADR follows the format: **Status / Context / Decision / Consequences / Alternatives Considered**.

### ADR-0001: Use Rust for the entire pipeline

- **Status:** Accepted.
- **Context:** The project requires reliable parallel raster processing, structured concurrency (many tiles fetched and processed in parallel), strong type safety for a pipeline with many enumerated states, and predictable resource use. The original prototype used Python.
- **Decision:** Implement the entire pipeline (acquisition, indices, masks, patches, scoring, legal context, VLM orchestration, evidence packet generation, dashboard) in Rust. Call out to non-Rust services (llama-server, Python sidecars for foundation-model export, training scripts) over HTTP or via FFI.
- **Consequences:**
  - + Compile-time enforcement of the type system below.
  - + Predictable memory and concurrency.
  - + Single binary deployment.
  - − Geospatial library ecosystem in Rust is thinner than Python's. Mitigated by `gdal`, `geo`, `geozero`, `proj`, `ndarray`.
  - − ML training stays in Python. Crossing the boundary is via ONNX Runtime + HTTP.
- **Alternatives:** Pure Python (rejected: weaker type system, GIL); Go (rejected: weaker numerical ecosystem); Rust + PyO3 embedded Python (rejected: deployment complexity).

### ADR-0002: VLM is invoked via HTTP to llama-server, not embedded

- **Status:** Accepted.
- **Context:** LFM2-VL / LFM2.5-VL has GGUF quantizations (Q4_0, Q8_0, F16) and is supported by llama.cpp's `llama-server`. There are Rust bindings (`llama-cpp-rs`) but they bind to a moving target and require GPU configuration in-process.
- **Decision:** Run `llama-server` as a sidecar process exposing the OpenAI-compatible HTTP API. The Rust pipeline calls it via `reqwest`. Use llama-server's `--json-schema` (GBNF) for structured output.
- **Consequences:**
  - + Decouples Rust build from llama.cpp build.
  - + Can swap models or hardware without recompiling.
  - + Easy to mock for tests.
  - − Adds a network hop (~1 ms locally, irrelevant).
  - − Process orchestration becomes a deployment concern (handled by `docker-compose.yml`).
- **Alternatives:** `llama-cpp-rs` embedded (rejected: build fragility); `candle` Rust-native inference (rejected: LFM2 architecture support uncertain at time of writing); ONNX Runtime (rejected: VLM ONNX export immature).

### ADR-0003: SQLite for persistence, with Postgres/PostGIS as the documented upgrade path

- **Status:** Accepted.
- **Context:** The MVP runs on a single developer machine. Production deployment may need spatial indexing and concurrent writes. Both the Rust ecosystem (`sqlx`, `rusqlite`) and SQLite itself support the MVP scale.
- **Decision:** Use SQLite with `sqlx` for compile-time-checked async queries. Store geometry as GeoJSON `TEXT` (not WKB, not SpatiaLite). Document the migration to Postgres/PostGIS as a non-MVP upgrade.
- **Consequences:**
  - + Zero-setup deployment.
  - + `sqlx` provides query-time type checking.
  - + GeoJSON-as-TEXT keeps the schema portable.
  - − No spatial indexes; spatial filtering loads geometry into memory.
  - − Single-writer.
- **Alternatives:** PostGIS from the start (rejected: setup overhead for MVP); DuckDB (rejected: weaker async story for serving).

### ADR-0004: Foundation-model embeddings via ONNX Runtime, not via Python sidecar at inference time

- **Status:** Accepted.
- **Context:** Clay v1 and Prithvi-EO-2.0 are PyTorch models. We want their embeddings in our Rust pipeline at inference time. Calling Python over IPC adds latency and a runtime dependency.
- **Decision:** Export the chosen foundation model to ONNX once (Python script in `tools/`). Use the `ort` crate (ONNX Runtime bindings) to load and run it from Rust. Document the export procedure in `tools/export_foundation_model.py`.
- **Consequences:**
  - + Pure Rust at inference time.
  - + CPU and CUDA execution providers available.
  - − ONNX export of vision transformers requires care (dynamic shapes, attention masks).
  - − If the model architecture changes, re-export needed.
- **Alternatives:** Python HTTP sidecar (rejected: latency, deployment complexity); skip embeddings (rejected: too valuable a signal).

### ADR-0005: The VLM is the structured narrator and asymmetric judge, never the primary detector

- **Status:** Accepted.
- **Context:** A 450M–1.6B general-purpose VLM is trained on natural RGB images. Sentinel-2 false-color, NIR, SWIR, and index-difference imagery are out-of-distribution. Existing benchmarks (GEOBench-VLM, VRSBench) show small VLMs performing near random on remote-sensing reasoning.
- **Decision:**
  1. The VLM is invoked **only after** a deterministic candidate alert exists.
  2. The VLM is given **only RGB inputs** and **numeric features as text**, never multispectral panels.
  3. The VLM can **demote** an alert (raise false-positive risks) but cannot **promote** above the deterministic ceiling.
  4. The VLM has seven narrowly-scoped roles (§18); no role asks it to perform primary detection.
- **Consequences:**
  - + The VLM operates in-distribution.
  - + Failures of the VLM degrade gracefully (alert defaults to deterministic findings).
  - + Reviewer trust is preserved.
  - − Some "wow factor" of VLM-as-detector is lost.
- **Alternatives:** VLM as primary detector (rejected per benchmark evidence); VLM unused (rejected: misses the structured-narration value).

### ADR-0006: JSON-schema-constrained output via llama-server `--json-schema` (GBNF)

- **Status:** Accepted.
- **Context:** All seven VLM roles must produce machine-parseable structured outputs. Free-form output is not acceptable in a pipeline.
- **Decision:** All VLM calls use llama-server's `--json-schema` flag with a GBNF-compiled schema. The Rust client validates the response against the same schema using `jsonschema` crate. On schema violation, retry once with a stricter prompt; on second failure, default to a "VLM unavailable" record.
- **Consequences:**
  - + Syntactic validity is guaranteed.
  - + Easy to evolve the schema.
  - − Semantic correctness is not guaranteed (covered by the asymmetric-judge rule).
  - − GBNF has known coverage gaps for complex schemas; we keep schemas flat.
- **Alternatives:** Function-calling API (rejected: not stable across llama.cpp versions); free-form + regex parsing (rejected: brittle).

### ADR-0007: Use real geographic overlay data even though imagery is synthetic

- **Status:** Accepted.
- **Context:** The legal-context engine is the project's most defensible value-add. SimSat imagery is synthetic but legal/protected-area data is real and freely available (WDPA, RAISG, OSM, GFW boundaries).
- **Decision:** Source real GeoJSON for protected areas, concessions, indigenous lands, roads, and settlements for the demonstration AOIs. SimSat tiles are positioned to overlap real protected areas. The legal-context engine is production-grade from day one.
- **Consequences:**
  - + The legal layer is real, not simulated.
  - + The demo is materially convincing.
  - − Some coordinate systems and projection care required.
- **Alternatives:** Synthetic boundaries (rejected: defeats the point).

### ADR-0008: Patches below 0.25 ha (25 pixels at S2 10 m) do not generate alerts

- **Status:** Accepted.
- **Context:** Shape statistics on connected components below ~25 pixels are dominated by quantization. Operational systems use floors of 0.2 ha (RADD), 3 ha (DETER-B), 6.25 ha (PRODES), 0.09 ha (JRC TMF, Landsat 30 m).
- **Decision:** Minimum mapping unit for alert generation is 0.25 ha (25 connected pixels at 10 m). Smaller patches are stored as "sub-MMU" candidates for future multi-observation accumulation but never raise `watch` or above.
- **Consequences:**
  - + Defensible.
  - + Avoids spurious alerts from co-registration drift.
  - − Some real small-scale clearing is missed.
- **Alternatives:** 0.05 ha (rejected per shape-statistics noise floor); 1.0 ha (rejected: too coarse for the demonstration).

### ADR-0009: Multi-observation confirmation is part of the schema from day one, even if not enforced in the MVP

- **Status:** Accepted.
- **Context:** Operational systems (GLAD-S2, RADD, DIST-ALERT) build confidence through repeated observations. The MVP runs on a small SimSat sample where N≥2 detections may be unavailable, but the schema must support it.
- **Decision:** Every Alert has a `detection_count` field and a `state` field with values including `awaiting_second_observation`. The MVP issues alerts at `detection_count=1` but documents the upgrade to require ≥2 for `enforcement_review`. The data model never has to change.
- **Consequences:**
  - + Future-proof schema.
  - + Honest framing (the MVP issues lower-confidence alerts).
- **Alternatives:** Defer entirely (rejected: schema migration risk).

### ADR-0010: The dashboard is server-rendered Rust (axum + askama + htmx) for the MVP

- **Status:** Accepted.
- **Context:** Streamlit is Python. egui/dioxus are Rust but require WebAssembly tooling. axum + askama (templates) + htmx (interactivity) is pure Rust on the server with minimal JavaScript.
- **Decision:** axum HTTP server, askama templates, htmx for interactions, MapLibre GL JS for the map (loaded from CDN). React/TypeScript is the documented upgrade path if richer UI is needed later.
- **Consequences:**
  - + Single Rust binary.
  - + No build step for the frontend.
  - − Dashboard interactivity is limited compared to a SPA.
- **Alternatives:** React/TypeScript (rejected for MVP scope); egui native (rejected: not browser-accessible); Streamlit (rejected: separate Python runtime).

### ADR-0011: All evidence packets are content-hashed and optionally signed

- **Status:** Accepted.
- **Context:** Evidence packets are the project's product. They must be tamper-evident.
- **Decision:** Every file in an evidence packet is SHA-256 hashed. The packet manifest is itself hashed. An optional Ed25519 signature is computed if a signing key is configured. Hashes and signatures are stored in `manifest.json` and in the `audit_log` table.
- **Consequences:**
  - + Tamper-evidence.
  - + Reproducibility audit.
  - − Adds a few hundred microseconds per packet.
- **Alternatives:** No hashing (rejected: undermines evidentiary claim).

### ADR-0012: Synthetic-data testing is structured around named scenarios, not precision/recall numbers

- **Status:** Accepted.
- **Context:** Reporting precision/recall on SimSat-only data is misleading. But the pipeline still needs validation.
- **Decision:** Define a fixed set of named test scenarios (Appendix B) with hand-crafted SimSat inputs and expected pipeline routing (which alert level, which disturbance type, which false-positive risks). The MVP's success criterion on synthetic data is "scenario routing is correct," not "precision is X."
- **Consequences:**
  - + Honest claims.
  - + Reproducible tests.
  - − Cannot publish a precision/recall number until real-data port.
- **Alternatives:** Report synthetic precision/recall (rejected: misleading).

### ADR-0013: Configuration is TOML, with environment overrides

- **Status:** Accepted.
- **Context:** Standard Rust ecosystem practice (`config-rs`, `figment`).
- **Decision:** Configuration in `config/forest-guardian.toml`, profile-specific overrides via `FG_PROFILE`, secrets via environment variables prefixed `FG_`.
- **Alternatives:** YAML, JSON. TOML chosen for Rust idiom.

### ADR-0014: All async I/O on Tokio; CPU-bound work via `rayon` or `tokio::task::spawn_blocking`

- **Status:** Accepted.
- **Decision:** Tokio is the runtime. Image processing, ndarray work, and ONNX inference run inside `spawn_blocking` or use `rayon` for parallelism. HTTP, DB, file I/O are async.
- **Alternatives:** async-std (rejected: smaller ecosystem).

### ADR-0015: Errors via `thiserror` in libraries, `anyhow` in binaries; all errors have categories

- **Status:** Accepted.
- **Decision:** Library crates expose typed errors via `thiserror`. The CLI and server use `anyhow` for top-level error handling but inspect typed errors for retry/category logic. Errors are categorized: `Transient` (retry), `Permanent` (fail), `Validation` (return 400), `Internal` (log + 500).

### ADR-0016: VLM zero-shot first; fine-tuning is a separate, phased deliverable

- **Status:** Accepted.
- **Decision:** The MVP uses LFM2.5-VL with zero-shot prompts. Fine-tuning targets, datasets, and procedures are documented (§25) but not part of the MVP. Each role is shippable zero-shot before its LoRA is trained.
- **Consequences:**
  - + Pipeline runs without ML training infrastructure.
  - + Fine-tuning becomes an optimization, not a prerequisite.

### ADR-0017: Foundation model: prefer Clay v1; Prithvi-EO-2.0 as alternate; both behind a trait

- **Status:** Accepted.
- **Context:** Clay v1 is multi-platform and embedding-focused. Prithvi-EO-2.0 is HLS-trained and downstream-task-tuned. Either works for embedding-based change detection.
- **Decision:** Define a `FoundationEmbedder` trait. Implement Clay v1 first. Document Prithvi-EO-2.0 as a swappable alternate. The pipeline never names either directly.

### ADR-0018: The cloud / shadow handling on SimSat is best-effort; the real implementation is deferred to real-data port

- **Status:** Accepted.
- **Context:** SimSat may or may not generate realistic cloud/shadow artifacts. `s2cloudless` and Sen2Cor SCL are designed for real Sentinel-2 L2A.
- **Decision:** Honor SimSat's `cloud_cover` metadata. Implement a `CloudMasker` trait with a `MetadataThreshold` impl for the MVP and a documented `S2cloudless` impl behind a feature flag for the real-data port.

### ADR-0019: All Patch geometries stored as both pixel-space and lon/lat polygons

- **Status:** Accepted.
- **Decision:** A Patch carries a pixel-space polygon (relative to its observation), a lon/lat polygon, and a centroid in lon/lat. Conversions go through the observation's footprint affine transform. This avoids an entire class of "I had pixels but needed lon/lat" bugs.

### ADR-0020: The pipeline is idempotent on (Tile, T_current, T_baseline) triples

- **Status:** Accepted.
- **Decision:** Re-running the pipeline with identical inputs produces identical outputs (same hashes). Achieved by:
  1. Stable observation hashes (band order, dtype, content).
  2. Deterministic numerical operations (no parallel reductions on floats without sorted inputs).
  3. Fixed VLM seed and temperature in production runs.
  4. Pinned model versions in the run record.

---

## 4. Domain-Driven Design

### 4.1 Bounded Contexts

```
┌─────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Acquisition    │  │  Analysis        │  │  Detection       │
│  (SimSat I/O,   │→ │  (indices, masks │→ │  (patches,       │
│   observations) │  │   embeddings,    │  │   classifier,    │
│                 │  │   change maps)   │  │   scoring)       │
└─────────────────┘  └──────────────────┘  └────────┬─────────┘
                                                    │
                                                    ▼
┌─────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Reporting      │← │  Review          │← │  Reasoning       │
│  (daily         │  │  (audit log,     │  │  (VLM 7 roles,   │
│   summaries,    │  │   reviewer       │  │   legal context, │
│   exports)      │  │   workflow)      │  │   evidence)      │
└─────────────────┘  └──────────────────┘  └──────────────────┘
```

Each bounded context maps to one or more Rust crates. Crossing a boundary always uses domain types (no leaking of `serde_json::Value`, `ndarray` rasters across boundaries).

### 4.2 Aggregates

An **aggregate** is a cluster of objects treated as a unit for consistency. Only the aggregate root is referenced by other aggregates.

| Aggregate Root | Contained Entities | Contained Value Objects |
|---|---|---|
| `Aoi` | — | `Geometry`, `Jurisdiction`, `AoiType`, `Priority` |
| `Tile` | — | `LatLon`, `BBox`, `TileSize`, `Priority` |
| `Observation` | — | `BandStack`, `Footprint`, `CloudCover`, `Hash`, `BandSet` |
| `FeatureMapSet` | `FeatureMap` (per index) | `IndexType`, `Statistics` |
| `ChangeEvent` | `Patch` (many) | `DeltaStatistics`, `QualityScore`, `LossScore` |
| `Alert` | `VlmReport` (many), `EvidencePacket` | `AlertLevel`, `DisturbanceType`, `LegalContext`, `Score`, `Confidence` |
| `Review` | `ReviewDecision` (many) | `Reviewer`, `Decision`, `Notes` |
| `Run` | `AuditEntry` (many) | `RunId`, `Started`, `Finished`, `Outcome` |

### 4.3 Domain Events

Events are emitted at aggregate state transitions. They flow through an internal bus (`tokio::sync::broadcast`) for in-process subscribers (audit log, dashboard live updates).

```rust
pub enum DomainEvent {
    ObservationFetched { tile_id: TileId, observation_id: ObservationId, hash: Sha256 },
    FeatureMapsComputed { observation_id: ObservationId, types: Vec<IndexType> },
    EmbeddingsComputed { observation_id: ObservationId, model: ModelRef, dim: u32 },
    ChangeDetected { tile_id: TileId, change_event_id: ChangeEventId, patch_count: u32 },
    AlertRaised { alert_id: AlertId, level: AlertLevel, disturbance_score: Score },
    AlertVlmAnalyzed { alert_id: AlertId, role: VlmRole, concurrence: VlmConcurrence },
    AlertReviewed { alert_id: AlertId, reviewer: ReviewerId, decision: ReviewDecision },
    EvidencePacketSealed { alert_id: AlertId, manifest_hash: Sha256, signed: bool },
    RunStarted { run_id: RunId, profile: String },
    RunCompleted { run_id: RunId, outcome: RunOutcome },
}
```

### 4.4 Repositories

Each aggregate has a repository trait. Implementations use `sqlx`.

```rust
#[async_trait]
pub trait AlertRepository: Send + Sync {
    async fn insert(&self, alert: &Alert) -> Result<(), RepoError>;
    async fn get(&self, id: AlertId) -> Result<Option<Alert>, RepoError>;
    async fn list_by_status(&self, status: AlertStatus, limit: u32) -> Result<Vec<Alert>, RepoError>;
    async fn update_status(&self, id: AlertId, status: AlertStatus) -> Result<(), RepoError>;
    async fn append_vlm_report(&self, id: AlertId, report: &VlmReport) -> Result<(), RepoError>;
}
```

### 4.5 Domain Services

Logic that doesn't naturally belong to an aggregate.

| Service | Responsibility |
|---|---|
| `ScoringService` | Compute `disturbance_score` from patch + quality + legal inputs. Pure function. |
| `LegalContextService` | Overlay a Patch geometry against legal layers, produce `LegalContext`. |
| `VlmOrchestrator` | Decide which VLM roles to run for a given Alert; execute them; merge into the Alert. |
| `EvidenceBuilder` | Compose composite images, write the packet directory, hash files, optionally sign. |
| `ChangeDetector` | Compose feature maps + masks + patch extraction into a ChangeEvent. |
| `ConfirmationPolicy` | Given an Alert and historical detection counts, decide whether to escalate. |

### 4.6 Anti-Corruption Layers

External integrations (SimSat, llama-server, ONNX models, OSM/WDPA data sources) are wrapped in adapters that translate their types into domain types. Domain code never sees a raw `serde_json::Value` from llama-server or a raw `gdal::Dataset`.

---

## 5. System Architecture

### 5.1 Component Diagram

```
                    ┌───────────────────────┐
                    │   forest-guardian-cli │
                    └───────────┬───────────┘
                                │
                                ▼
        ┌─────────────────────────────────────────────────┐
        │       forest-guardian-pipeline (orchestrator)   │
        │  ┌─────────┬─────────┬─────────┬─────────────┐  │
        │  │ Acquire │ Analyze │ Detect  │ Reason      │  │
        │  │         │         │         │ (VLM)       │  │
        │  └────┬────┴────┬────┴────┬────┴──────┬──────┘  │
        └───────┼─────────┼─────────┼───────────┼─────────┘
                │         │         │           │
                ▼         ▼         ▼           ▼
        ┌──────────┐ ┌─────────┐ ┌──────┐ ┌──────────┐
        │ SimSat   │ │ ONNX    │ │ DB   │ │ llama-   │
        │ HTTP API │ │ Runtime │ │ (sx) │ │ server   │
        └──────────┘ │ (Clay)  │ └──┬───┘ │ (HTTP)   │
                     └─────────┘    │     └──────────┘
                                    ▼
                              ┌──────────┐
                              │ SQLite   │
                              └──────────┘

        ┌─────────────────────────────────────────────────┐
        │       forest-guardian-server (axum HTTP)        │
        │  HTML+htmx dashboard, JSON API, evidence files  │
        └─────────────────────────────────────────────────┘
```

### 5.2 Process Topology

For local development:

```yaml
# docker-compose.yml (illustrative)
services:
  simsat:                # provided by SimSat repo, ports 8000 + 9005
  llama-server:          # llama-cpp-python or llama.cpp release
    image: ghcr.io/ggerganov/llama.cpp:server
    command: -hf LiquidAI/LFM2-VL-1_6B-GGUF:Q8_0 --jinja --port 8080 --json-schema ${SCHEMA}
    ports: [8080]
  forest-guardian-server:
    build: .
    depends_on: [simsat, llama-server]
    ports: [3000]
```

### 5.3 Data Flow (per pipeline run)

1. CLI or scheduler triggers a Run for one or more Tiles.
2. **Acquire**: For each Tile, fetch T_current and T_baseline observations from SimSat. Persist to `observations` table and write band arrays to disk as Cloud-Optimized GeoTIFF (or NPY for MVP).
3. **Analyze**: For each Observation, compute FeatureMaps (NDVI, NDMI, NBR, BSI, NDWI). Compute Clay embeddings. Persist statistics; rasters to disk.
4. **Detect**: Compute DeltaMaps. Build CandidateMask. Extract Patches via connected components. Score each Patch deterministically. Run LightGBM classifier on each Patch (if model available). Persist as a ChangeEvent.
5. **Reason**: For each Patch above threshold, build an Alert. Resolve LegalContext. Invoke VLM roles (1, 2, 3 minimum). Merge results. Asymmetric judge rule applied.
6. **Evidence**: For Alerts at level ≥ `investigate`, build the EvidencePacket directory with composite images, JSON, hashes, and (optional) signature.
7. **Persist**: Alert, VlmReports, EvidencePacket references written. Domain events emitted.
8. **Audit**: Every step appends to `audit_log` with input hashes, code SHA, parameters.

### 5.4 Deployment Profiles

| Profile | Purpose | DB | VLM Backend | Foundation Model |
|---|---|---|---|---|
| `dev` | Local development | SQLite file | llama-server localhost | ONNX CPU |
| `test` | CI / unit tests | SQLite in-memory | mock VLM | mock embedder |
| `demo` | Hackathon demo | SQLite file | llama-server localhost | ONNX CPU |
| `prod` | (future) | Postgres/PostGIS | llama-server cluster | ONNX CUDA |

---

## 6. Rust Workspace Layout

```
forest-guardian/
├── Cargo.toml                    # workspace
├── rust-toolchain.toml           # pin to stable 1.83+
├── BUILD.md                      # this document
├── README.md
├── config/
│   ├── forest-guardian.toml
│   ├── dev.toml
│   ├── demo.toml
│   └── prod.toml
├── data/
│   ├── aois/                     # GeoJSON (real WDPA, OSM)
│   ├── observations/             # COGs/NPYs
│   ├── feature_maps/
│   ├── embeddings/
│   ├── alert_packets/
│   └── labels/                   # for fine-tuning
├── migrations/                   # sqlx migrations
│   ├── 0001_initial.sql
│   ├── 0002_aoi_legal_layers.sql
│   └── ...
├── schemas/                      # JSON Schemas (canonical)
│   ├── vlm_role_01_triage.schema.json
│   ├── vlm_role_02_classification.schema.json
│   ├── vlm_role_03_narrative.schema.json
│   ├── vlm_role_04_map_reading.schema.json
│   ├── vlm_role_05_chat.schema.json
│   ├── vlm_role_06_ground_photo.schema.json
│   ├── vlm_role_07_daily_summary.schema.json
│   ├── alert.schema.json
│   ├── evidence_manifest.schema.json
│   └── domain_event.schema.json
├── crates/
│   ├── fg-core/                  # domain types only, no I/O
│   ├── fg-simsat/                # SimSat client
│   ├── fg-raster/                # ndarray + GDAL wrappers, indices
│   ├── fg-detect/                # masks, patches, scoring
│   ├── fg-embed/                 # ONNX foundation model
│   ├── fg-classify/              # LightGBM bindings
│   ├── fg-legal/                 # geographic overlay
│   ├── fg-vlm/                   # llama-server client + 7 roles
│   ├── fg-evidence/              # packet builder, hashing, signing
│   ├── fg-db/                    # sqlx repositories
│   ├── fg-pipeline/              # orchestrator: runs the whole thing
│   ├── fg-server/                # axum HTTP server + dashboard
│   ├── fg-cli/                   # command-line entrypoint
│   └── fg-test-fixtures/         # shared test scenarios + helpers
├── tools/
│   ├── export_foundation_model.py
│   ├── train_lightgbm.py
│   ├── build_finetune_dataset.py
│   ├── train_lora.py
│   └── eval_vlm_roles.py
├── tests/
│   ├── e2e/
│   └── scenarios/
└── xtask/                        # repo automation (build, lint, schema-gen)
```

### 6.1 Crate Dependency Graph

```
fg-cli ─┐
        ├─→ fg-pipeline ─→ {fg-simsat, fg-raster, fg-detect, fg-embed,
fg-server ─┘                fg-classify, fg-legal, fg-vlm, fg-evidence}
                              │     │     │     │     │     │     │
                              ▼     ▼     ▼     ▼     ▼     ▼     ▼
                            fg-core (used by all; depends on nothing)

                            fg-db    used by   fg-pipeline, fg-server
                            fg-test-fixtures   used by tests only
```

`fg-core` is the most-depended-on crate and depends only on `serde`, `chrono`, `uuid`, `geo-types`, `thiserror`, `schemars`. No I/O.

---

## 7. Crate Specifications

For each crate: purpose, public API outline, dependencies, success criteria.

### 7.1 `fg-core`

**Purpose:** Domain types, IDs, value objects, enums, error types. No I/O. No async. No third-party data formats (no `gdal`, no `sqlx`, no `reqwest`).

**Public API:**

```rust
pub mod ids;            // TileId, ObservationId, AlertId, RunId, ReviewerId, ChangeEventId, PatchId, VlmReportId
pub mod geo;            // LatLon, BBox, Footprint, AffineTransform, Polygon (re-export geo_types)
pub mod bands;          // BandName, BandSet, BandStack (typed array)
pub mod indices;        // IndexType enum
pub mod masks;          // MaskKind, BoolMask
pub mod patches;        // Patch, PatchShape, PatchStatistics
pub mod scoring;        // Score, QualityScore, LossScore, DisturbanceScore, Confidence
pub mod alert;          // AlertLevel, AlertStatus, DisturbanceType
pub mod legal;          // LegalContext, LegalConclusion, Jurisdiction, AoiType
pub mod vlm;            // VlmRole, VlmReport, VlmConcurrence
pub mod time;           // re-export chrono with helper aliases
pub mod hash;           // Sha256 newtype
pub mod errors;         // CoreError
pub mod events;         // DomainEvent
pub mod ubiquitous;     // re-exports of common terms
```

**Dependencies:** `serde`, `serde_json`, `chrono`, `uuid`, `geo-types`, `thiserror`, `schemars`, `sha2`.

**Success criteria:**
- All domain types implement `Serialize + Deserialize + Clone + Debug + PartialEq`.
- All ID types are newtypes (`TileId(String)`, `AlertId(Uuid)`, etc.) with `FromStr`/`Display`.
- All enums implement `JsonSchema` for schema generation.
- No `unwrap()` outside tests.
- 90%+ test coverage.

### 7.2 `fg-simsat`

**Purpose:** Typed client for the SimSat HTTP API.

**Public API:**

```rust
pub struct SimSatClient { /* base URL, http client */ }

impl SimSatClient {
    pub fn new(base_url: Url) -> Self;
    pub async fn current_position(&self) -> Result<SatellitePosition, SimSatError>;
    pub async fn current_image_sentinel(&self, params: ImageParams) -> Result<SentinelImage, SimSatError>;
    pub async fn image_sentinel(&self, params: SentinelImageRequest) -> Result<SentinelImage, SimSatError>;
}

pub struct SentinelImageRequest {
    pub lon: f64,
    pub lat: f64,
    pub timestamp: chrono::DateTime<Utc>,
    pub bands: BandSet,                  // typed
    pub size_km: f64,
    pub return_type: ReturnType,         // Png | Array
    pub window_seconds: Option<u64>,
}

pub enum SentinelImage {
    Png { bytes: Vec<u8>, metadata: SentinelMetadata },
    Array { stack: BandStack, metadata: SentinelMetadata },
}

pub struct SentinelMetadata {
    pub image_available: bool,
    pub source_satellite: String,
    pub footprint: Footprint,
    pub cloud_cover: f32,
    pub source_image_datetime: DateTime<Utc>,
    pub simulation_timestamp: DateTime<Utc>,
}
```

**Dependencies:** `fg-core`, `reqwest`, `tokio`, `serde`, `chrono`, `bytes`, `ndarray`, `image`.

**Success criteria:**
- 100% of SimSat endpoints used by the pipeline are typed.
- `cargo test` includes a fake-server (wiremock) test for each endpoint.
- All 9 supported bands round-trip correctly (red, green, blue, nir, swir16, swir22, rededge1, rededge2, rededge3).
- Returns `BandStack` with explicit dtype handling (cast to f32 at boundary).

### 7.3 `fg-raster`

**Purpose:** Numerical operations over BandStacks: indices, deltas, statistics, GeoTIFF I/O.

**Public API:**

```rust
pub fn ndvi(stack: &BandStack) -> Result<FeatureMap, RasterError>;
pub fn ndmi(stack: &BandStack) -> Result<FeatureMap, RasterError>;
pub fn nbr(stack: &BandStack) -> Result<FeatureMap, RasterError>;
pub fn bsi(stack: &BandStack) -> Result<FeatureMap, RasterError>;
pub fn ndwi(stack: &BandStack) -> Result<FeatureMap, RasterError>;

pub fn delta(current: &FeatureMap, baseline: &FeatureMap) -> Result<FeatureMap, RasterError>;
pub fn statistics(map: &FeatureMap) -> FeatureMapStatistics;

pub mod io {
    pub fn write_cog(map: &FeatureMap, path: &Path) -> Result<(), RasterError>;
    pub fn read_cog(path: &Path) -> Result<FeatureMap, RasterError>;
    pub fn write_npy(stack: &BandStack, path: &Path) -> Result<(), RasterError>;
    pub fn read_npy(path: &Path) -> Result<BandStack, RasterError>;
}

pub mod render {
    pub fn rgb_chip(stack: &BandStack, stretch: Stretch) -> Result<DynamicImage, RasterError>;
    pub fn false_color(stack: &BandStack, kind: FalseColorKind) -> Result<DynamicImage, RasterError>;
    pub fn diverging_colormap(map: &FeatureMap, range: (f32, f32)) -> Result<DynamicImage, RasterError>;
}
```

**Dependencies:** `fg-core`, `ndarray`, `gdal`, `image`, `rayon`.

**Success criteria:**
- Index formulas exactly match Appendix A.
- Numerical reproducibility: identical input → identical output (no parallel-reduction nondeterminism).
- All operations use `f32` with `safe_div(num, den, eps=1e-6)`.
- COG output is openable by QGIS without warnings.
- RGB chip renders are perceptually reasonable on synthetic SimSat data (validated by visual inspection on test fixtures).

### 7.4 `fg-detect`

**Purpose:** Masks, patches, scoring, classifier integration.

**Public API:**

```rust
pub fn forest_baseline_mask(maps: &BaselineMaps, thresholds: &ForestThresholds) -> BoolMask;
pub fn water_mask(ndwi: &FeatureMap, ndvi: &FeatureMap) -> BoolMask;
pub fn candidate_loss_mask(input: &CandidateInputs, thresholds: &ChangeThresholds) -> BoolMask;

pub fn extract_patches(mask: &BoolMask, footprint: &Footprint, min_pixels: u32) -> Vec<Patch>;

pub fn score(patch: &Patch, deltas: &PatchDeltaStats, quality: &QualityInputs, legal: &LegalContext)
    -> DisturbanceScore;

pub fn classify(features: &PatchFeatureVector, model: &LightGBMModel) -> ClassifierOutput;

pub mod thresholds {
    pub fn for_biome(biome: Biome) -> (ForestThresholds, ChangeThresholds);
}
```

**Dependencies:** `fg-core`, `fg-raster`, `ndarray`, `imageproc` (connected components), `geo`.

**Success criteria:**
- Connected components match scipy.ndimage.label on identical inputs.
- Patch shape statistics (area, perimeter, compactness, elongation) within 1% of skimage.measure.regionprops on test fixtures.
- Scoring is a pure function with documented weights.
- Biome thresholds for at least 3 strata: humid_tropical, dry_forest, mangrove.

### 7.5 `fg-embed`

**Purpose:** Foundation-model embeddings via ONNX Runtime.

**Public API:**

```rust
#[async_trait]
pub trait FoundationEmbedder: Send + Sync {
    fn name(&self) -> &str;                      // "clay-v1" or "prithvi-eo-2"
    fn embedding_dim(&self) -> u32;
    fn input_resolution(&self) -> u32;
    fn input_bands(&self) -> &[BandName];
    async fn embed(&self, stack: &BandStack) -> Result<Embedding, EmbedError>;
}

pub struct ClayV1 { /* onnx session */ }
pub struct PrithviEo2 { /* onnx session */ }

pub fn anomaly_score(current: &Embedding, baseline: &Embedding) -> f32; // cosine distance

pub struct EmbeddingStore { /* persists to disk; LRU memory cache */ }
```

**Dependencies:** `fg-core`, `ort` (ONNX Runtime), `ndarray`, `tokio`.

**Success criteria:**
- Clay v1 ONNX export in `tools/export_foundation_model.py` produces a model that loads and runs in the Rust pipeline.
- Embedding dim matches the published model.
- Cosine distance is symmetric and in `[0, 2]`.
- Inference runs on CPU at acceptable latency for the demo (<2s per 256×256 patch).

### 7.6 `fg-classify`

**Purpose:** LightGBM classifier for disturbance type.

**Public API:**

```rust
pub struct LightGBMModel { /* loaded model */ }

impl LightGBMModel {
    pub fn load(path: &Path) -> Result<Self, ClassifyError>;
    pub fn predict(&self, features: &PatchFeatureVector) -> ClassifierOutput;
}

pub struct PatchFeatureVector {
    pub mean_ndvi_current: f32,
    pub mean_ndvi_baseline: f32,
    pub mean_delta_ndvi: f32,
    // ... full list documented in §20
}

pub struct ClassifierOutput {
    pub disturbance_type: DisturbanceType,
    pub probabilities: HashMap<DisturbanceType, f32>,
    pub confidence: f32,
}
```

**Dependencies:** `fg-core`, `lightgbm3` or FFI bindings.

**Success criteria:**
- Loads a `lgb_model.txt` (text-format LightGBM model) trained by `tools/train_lightgbm.py`.
- Predicts in <1 ms per patch.
- The MVP ships with a model trained on synthetic-only labeled data and is documented as such.

### 7.7 `fg-legal`

**Purpose:** Geographic overlay against legal/protected/permit/indigenous/road/settlement layers.

**Public API:**

```rust
pub struct LegalContextEngine { /* loaded layers */ }

impl LegalContextEngine {
    pub fn load_from_dir(dir: &Path) -> Result<Self, LegalError>;
    pub fn evaluate(&self, patch_geometry: &Polygon) -> LegalContext;
}

// LegalContext defined in fg-core; reproduced here for reference:
pub struct LegalContext {
    pub inside_protected_area: bool,
    pub overlaps_known_permit: bool,
    pub inside_logging_concession: bool,
    pub inside_indigenous_or_community_land: bool,
    pub distance_to_known_road_m: Option<f64>,
    pub distance_to_settlement_m: Option<f64>,
    pub possible_unauthorized_activity: bool,
    pub legal_conclusion: LegalConclusion,
    pub matched_layer_ids: Vec<String>,
}
```

**Dependencies:** `fg-core`, `geo`, `geozero`, `geojson`, `rstar` (spatial index).

**Success criteria:**
- Loads WDPA, RAISG, OSM-derived GeoJSON.
- Spatial queries use R-tree index; <10 ms per patch for 100k geometries.
- Decision logic matches the rule table in §16.

### 7.8 `fg-vlm`

**Purpose:** llama-server client + the seven VLM roles + asymmetric judgment merge.

**Public API:**

```rust
pub struct VlmClient { /* http client, model ref */ }

impl VlmClient {
    pub fn new(base_url: Url, model_ref: ModelRef) -> Self;
    pub async fn run_role(&self, role: VlmRole, input: VlmInput) -> Result<VlmReport, VlmError>;
}

pub struct VlmInput {
    pub system_prompt: String,
    pub user_text: String,
    pub images: Vec<DynamicImage>,
    pub schema: serde_json::Value,
    pub temperature: f32,                  // production default 0.2
    pub max_tokens: u32,
}

pub struct VlmOrchestrator { /* uses VlmClient */ }

impl VlmOrchestrator {
    pub async fn run_for_alert(&self, alert: &mut Alert, /* role plan */) -> Result<(), VlmError>;
    pub async fn run_role_only(&self, role: VlmRole, alert: &Alert) -> Result<VlmReport, VlmError>;
}

pub fn apply_asymmetric_judgment(
    deterministic_alert: &mut Alert,
    triage: &TriageReport,
    classification: &ClassificationConfirmation,
);
```

**Dependencies:** `fg-core`, `reqwest`, `image`, `base64`, `serde_json`, `jsonschema`.

**Success criteria:**
- Each role has a frozen prompt (versioned in `crates/fg-vlm/prompts/`).
- Each role outputs valid JSON 99%+ of the time on synthetic test inputs.
- Asymmetric judgment never raises an alert level above the deterministic ceiling.
- Mock client implementation (`MockVlmClient`) is used in tests.

### 7.9 `fg-evidence`

**Purpose:** Build the evidence packet directory; hash; sign; render.

**Public API:**

```rust
pub struct EvidenceBuilder { /* fonts, colormaps, signing key */ }

impl EvidenceBuilder {
    pub fn build(&self, alert: &Alert, observations: &ObservationPair, maps: &FeatureMapSetPair)
        -> Result<EvidencePacket, EvidenceError>;
}

pub struct EvidencePacket {
    pub alert_id: AlertId,
    pub directory: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest_hash: Sha256,
    pub signed: bool,
}

pub fn render_eight_panel_composite(/* ... */) -> Result<DynamicImage, EvidenceError>;
pub fn render_rgb_before_after_chip(/* ... */) -> Result<DynamicImage, EvidenceError>;
pub fn render_legal_overlay(/* ... */) -> Result<DynamicImage, EvidenceError>;
```

**Dependencies:** `fg-core`, `fg-raster`, `image`, `imageproc`, `ed25519-dalek`, `sha2`.

**Success criteria:**
- The packet directory matches §21 exactly.
- All file hashes are reproducible.
- Optional signature verifies with the corresponding public key.

### 7.10 `fg-db`

**Purpose:** sqlx repositories implementing the traits in `fg-core`.

**Public API:**

```rust
pub struct Database { pool: SqlitePool }

impl Database {
    pub async fn connect(url: &str) -> Result<Self, DbError>;
    pub async fn migrate(&self) -> Result<(), DbError>;
    pub fn aoi_repo(&self) -> AoiRepo;
    pub fn tile_repo(&self) -> TileRepo;
    pub fn observation_repo(&self) -> ObservationRepo;
    pub fn change_event_repo(&self) -> ChangeEventRepo;
    pub fn alert_repo(&self) -> AlertRepo;
    pub fn vlm_report_repo(&self) -> VlmReportRepo;
    pub fn review_repo(&self) -> ReviewRepo;
    pub fn audit_repo(&self) -> AuditRepo;
    pub fn run_repo(&self) -> RunRepo;
}
```

**Dependencies:** `fg-core`, `sqlx` (sqlite, runtime-tokio, macros), `tokio`.

**Success criteria:**
- All repositories pass an in-memory SQLite test suite.
- Migrations apply cleanly from scratch and from prior versions.
- All queries use `query!` / `query_as!` for compile-time checking.

### 7.11 `fg-pipeline`

**Purpose:** Orchestrate one Run end-to-end.

**Public API:**

```rust
pub struct Pipeline {
    simsat: SimSatClient,
    db: Database,
    embedder: Box<dyn FoundationEmbedder>,
    classifier: Option<LightGBMModel>,
    legal: LegalContextEngine,
    vlm: VlmOrchestrator,
    evidence: EvidenceBuilder,
    config: PipelineConfig,
}

impl Pipeline {
    pub async fn run_tile(&self, tile_id: TileId, run_ctx: &RunContext) -> Result<RunOutcome, PipelineError>;
    pub async fn run_aoi(&self, aoi_id: AoiId, run_ctx: &RunContext) -> Result<RunOutcome, PipelineError>;
    pub async fn run_all(&self, run_ctx: &RunContext) -> Result<RunOutcome, PipelineError>;
}
```

**Dependencies:** all above.

**Success criteria:**
- A complete run on 5 demo tiles takes < 5 minutes on a laptop.
- Idempotent: re-run with same inputs produces same outputs.
- Domain events emitted at every aggregate transition.

### 7.12 `fg-server`

**Purpose:** axum HTTP server. Two surfaces: HTML dashboard (htmx) and JSON API.

**Public API:** HTTP routes (§11).

**Dependencies:** `fg-core`, `fg-db`, `fg-pipeline`, `axum`, `askama`, `tower`, `tower-http`, `serde_json`.

**Success criteria:**
- All routes typed with axum extractors.
- Server-sent events stream domain events to the dashboard.
- Static evidence files served with content-type and cache headers.

### 7.13 `fg-cli`

**Purpose:** Command-line entrypoint for local runs and admin tasks.

**Subcommands:**

```
fg run              # run pipeline once
fg run --tile <id>  # one tile
fg backfill         # build historical observations
fg ingest-aois      # load GeoJSON into DB
fg train-lgbm       # invoke Python training (subprocess)
fg vlm-test         # exercise each VLM role on a fixture
fg packet-verify    # verify hashes and signature on an evidence packet
fg serve            # start the axum server
fg migrate          # apply DB migrations
```

**Dependencies:** `fg-pipeline`, `fg-server`, `clap`, `tokio`.

### 7.14 `fg-test-fixtures`

**Purpose:** Shared test scenarios from Appendix B; helpers for constructing valid domain objects in tests.

---

## 8. Type System (Complete)

This section is the single source of truth for all types. Crates implement these definitions verbatim.

### 8.1 IDs (`fg_core::ids`)

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileId(pub String);                  // e.g., "BRA_AMZ_RESERVE_001_00042"

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AoiId(pub Uuid);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObservationId(pub i64);              // sqlite autoincrement

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChangeEventId(pub i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatchId(pub i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlertId(pub Uuid);                   // e.g., "ALERT_<uuidv7>"

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VlmReportId(pub i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReviewerId(pub Uuid);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub Uuid);
```

### 8.2 Geographic (`fg_core::geo`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LatLon { pub lat: f64, pub lon: f64 }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub min_lon: f64, pub min_lat: f64,
    pub max_lon: f64, pub max_lat: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Footprint {
    pub bbox: BBox,
    pub size_km: f32,
    pub crs: String,                            // "EPSG:4326"
    pub affine: AffineTransform,                // pixel ↔ lon/lat
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AffineTransform {
    pub a: f64, pub b: f64, pub c: f64,         // [a b c]
    pub d: f64, pub e: f64, pub f: f64,         // [d e f]
}                                                // x_geo = a*col + b*row + c; y_geo = d*col + e*row + f

pub use geo_types::Polygon as Polygon;
pub use geo_types::Point as Point;
```

### 8.3 Bands (`fg_core::bands`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BandName {
    Red, Green, Blue,
    Nir,                                        // ~842 nm (S2 B8) or ~865 nm (S2 B8A)
    Swir16,                                     // ~1610 nm (S2 B11)
    Swir22,                                     // ~2190 nm (S2 B12)
    RedEdge1,                                   // ~705 nm (S2 B5)
    RedEdge2,                                   // ~740 nm (S2 B6)
    RedEdge3,                                   // ~783 nm (S2 B7)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BandSet(pub Vec<BandName>);

#[derive(Clone, Debug)]
pub struct BandStack {
    pub bands: Vec<BandName>,                   // in axis order
    pub data: ndarray::Array3<f32>,             // (band, row, col), normalized to [0,1]
    pub footprint: Footprint,
    pub source_dtype: SourceDtype,              // Uint16 | Float32 | Uint8
    pub scale_factor: f32,                      // multiplier applied at ingest
    pub no_data: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceDtype { Uint8, Uint16, Float32 }
```

`BandStack` always normalizes to `f32` in `[0, 1]` at ingest. Index code never sees integer rasters.

### 8.4 Indices and Maps (`fg_core::indices`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IndexType {
    Ndvi, Ndmi, Nbr, Bsi, Ndwi, Ndre,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureMapStatistics {
    pub min: f32, pub max: f32,
    pub mean: f32, pub std: f32,
    pub p10: f32, pub p50: f32, pub p90: f32,
    pub no_data_fraction: f32,
}

#[derive(Clone, Debug)]
pub struct FeatureMap {
    pub kind: FeatureMapKind,
    pub data: ndarray::Array2<f32>,
    pub footprint: Footprint,
    pub statistics: FeatureMapStatistics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureMapKind {
    Index(IndexType),
    Delta(IndexType),
    Z(IndexType),
}
```

### 8.5 Masks (`fg_core::masks`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaskKind {
    ForestBaseline,
    Water,
    Cloud,
    Shadow,
    NoData,
    CandidateLoss,
}

#[derive(Clone, Debug)]
pub struct BoolMask {
    pub kind: MaskKind,
    pub data: ndarray::Array2<bool>,
    pub footprint: Footprint,
    pub true_fraction: f32,
}
```

### 8.6 Patches (`fg_core::patches`)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Patch {
    pub id: PatchId,
    pub change_event_id: ChangeEventId,
    pub component_index: u32,
    pub area_ha: f32,
    pub pixel_count: u32,
    pub centroid: LatLon,
    pub bbox: BBox,
    pub geometry_geojson: String,               // Polygon, EPSG:4326
    pub pixel_geometry: PixelPolygon,           // relative to observation
    pub shape: PatchShape,
    pub statistics: PatchPixelStatistics,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatchShape {
    pub perimeter_m: f32,
    pub compactness: f32,                       // 4πA/P²
    pub elongation: f32,                        // major/minor axis length
    pub solidity: f32,                          // area / convex hull area
    pub edge_sharpness: f32,                    // gradient magnitude on boundary
    pub orientation_deg: f32,                   // major axis angle
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatchPixelStatistics {
    pub mean_ndvi_current: f32,
    pub mean_ndvi_baseline: f32,
    pub mean_delta_ndvi: f32,
    pub mean_nbr_current: f32,
    pub mean_nbr_baseline: f32,
    pub mean_delta_nbr: f32,
    pub mean_bsi_current: f32,
    pub mean_delta_bsi: f32,
    pub mean_ndmi_current: f32,
    pub mean_delta_ndmi: f32,
    pub forest_baseline_fraction: f32,
    pub water_fraction: f32,
    pub cloud_fraction: f32,
    pub embedding_anomaly_score: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PixelPolygon { pub points: Vec<(u32, u32)> }
```

### 8.7 Scoring (`fg_core::scoring`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score(pub f32);                      // [0, 1]

impl Score {
    pub fn new(v: f32) -> Result<Self, CoreError> {
        if (0.0..=1.0).contains(&v) { Ok(Score(v)) } else { Err(CoreError::ScoreOutOfRange(v)) }
    }
    pub fn clamped(v: f32) -> Self { Score(v.clamp(0.0, 1.0)) }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct LossScore(pub Score);
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct QualityScore(pub Score);
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DisturbanceScore(pub Score);
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Confidence(pub Score);
```

### 8.8 Alert (`fg_core::alert`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertLevel { None, Watch, Investigate, EnforcementReview }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertStatus {
    New, Triaged, AwaitingSecondObservation,
    HumanReviewed, SentToAuthority,
    DismissedFalsePositive, ConfirmedDisturbance, Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DisturbanceType {
    None,
    ClearCut, SelectiveLogging, LoggingRoad,
    BurnScar, FloodOrWaterChange, AgricultureOrHarvest,
    Mining, Windthrow, StormDamage,
    CloudShadowArtifact, UnknownDisturbance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FalsePositiveRisk {
    Cloud, Shadow, Water, SeasonalCropHarvest,
    Fire, StormDamage, SensorOrTileArtifact,
    TemporalMismatch, GeoregistrationMismatch, Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecommendedAction {
    NoAction,
    MonitorNextClearObservation,
    RequestHigherResolutionFollowup,
    SendToHumanReview,
    SendToEnforcementReview,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Alert {
    pub id: AlertId,
    pub tile_id: TileId,
    pub change_event_id: ChangeEventId,
    pub primary_patch_id: PatchId,
    pub all_patch_ids: Vec<PatchId>,
    pub level: AlertLevel,
    pub status: AlertStatus,
    pub disturbance_type: DisturbanceType,
    pub possible_unauthorized_activity: bool,
    pub area_estimate_ha: f32,
    pub confidence: Confidence,
    pub deterministic_score: DisturbanceScore,
    pub vlm_concurrence: Option<VlmConcurrence>,
    pub legal_context: LegalContext,
    pub false_positive_risks: Vec<FalsePositiveRisk>,
    pub recommended_action: RecommendedAction,
    pub detection_count: u32,
    pub vlm_report_ids: Vec<VlmReportId>,
    pub evidence_packet_path: Option<PathBuf>,
    pub provenance: Provenance,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    pub run_id: RunId,
    pub code_sha: String,                       // git SHA at run time
    pub config_hash: Sha256,
    pub current_observation_id: ObservationId,
    pub baseline_observation_id: ObservationId,
    pub model_refs: Vec<ModelRef>,
    pub thresholds_used: ThresholdSet,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRef {
    pub kind: ModelKind,                        // Vlm | Embedder | Classifier
    pub name: String,                           // "LiquidAI/LFM2-VL-1.6B-GGUF"
    pub quant: Option<String>,                  // "Q8_0"
    pub version: String,
    pub hash: Option<Sha256>,
}
```

### 8.9 Legal (`fg_core::legal`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LegalConclusion {
    NotAssessed,
    NotALegalConclusion,
    RequiresAuthorityReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AoiType {
    ProtectedArea,
    ForestReserve,
    LoggingConcession,
    IndigenousLand,
    CommunityForest,
    KnownAgriculture,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LegalContext {
    pub inside_protected_area: bool,
    pub overlaps_known_permit: bool,
    pub inside_logging_concession: bool,
    pub inside_indigenous_or_community_land: bool,
    pub distance_to_known_road_m: Option<f64>,
    pub distance_to_settlement_m: Option<f64>,
    pub possible_unauthorized_activity: bool,
    pub legal_conclusion: LegalConclusion,
    pub matched_layer_ids: Vec<String>,
}
```

### 8.10 VLM (`fg_core::vlm`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VlmRole {
    Triage = 1,
    ClassificationConfirmation = 2,
    Narrative = 3,
    MapReading = 4,
    ReviewerChat = 5,
    GroundPhotoCheck = 6,
    DailySummary = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VlmConcurrence {
    Concur, ConcurWithCaveat, Neutral, Disagree, Unable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VlmReport {
    pub id: VlmReportId,
    pub alert_id: AlertId,
    pub role: VlmRole,
    pub model: ModelRef,
    pub prompt_hash: Sha256,
    pub input_image_hashes: Vec<Sha256>,
    pub response_json: serde_json::Value,
    pub schema_valid: bool,
    pub schema_errors: Vec<String>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub seed: Option<u64>,
    pub latency_ms: u64,
    pub created_at: DateTime<Utc>,
}
```

### 8.11 Errors (`fg_core::errors`)

```rust
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("score out of range: {0}")] ScoreOutOfRange(f32),
    #[error("invalid bbox: {0:?}")] InvalidBBox(BBox),
    #[error("invalid band set: {0}")] InvalidBandSet(String),
    #[error("invalid hash: {0}")] InvalidHash(String),
    // ... per-module variants
}

pub trait ErrorCategory {
    fn category(&self) -> ErrorCat;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCat { Transient, Permanent, Validation, Internal }
```

---

## 9. Database Schema (SQLite DDL)

Migrations live in `migrations/`. The full DDL:

```sql
-- 0001_initial.sql

CREATE TABLE aoi (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    country         TEXT,
    jurisdiction    TEXT,
    aoi_type        TEXT NOT NULL CHECK (aoi_type IN
                    ('protected_area','forest_reserve','logging_concession',
                     'indigenous_land','community_forest','known_agriculture','other')),
    geometry_geojson TEXT NOT NULL,
    priority        TEXT NOT NULL CHECK (priority IN ('low','medium','high','critical')),
    publication_policy TEXT NOT NULL DEFAULT 'public'
                    CHECK (publication_policy IN ('public','restricted','embargoed')),
    created_at      TEXT NOT NULL
);

CREATE TABLE tiles (
    id              TEXT PRIMARY KEY,
    aoi_id          TEXT NOT NULL REFERENCES aoi(id),
    center_lon      REAL NOT NULL,
    center_lat      REAL NOT NULL,
    size_km         REAL NOT NULL,
    bbox_json       TEXT NOT NULL,
    biome           TEXT,
    priority        TEXT NOT NULL,
    active          INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_tiles_aoi ON tiles(aoi_id);
CREATE INDEX idx_tiles_active ON tiles(active);

CREATE TABLE observations (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    tile_id               TEXT NOT NULL REFERENCES tiles(id),
    requested_timestamp   TEXT NOT NULL,
    image_datetime        TEXT,
    source_satellite      TEXT,
    footprint_json        TEXT NOT NULL,
    cloud_cover           REAL,
    image_available       INTEGER NOT NULL,
    window_seconds        INTEGER,
    bands_json            TEXT NOT NULL,
    rgb_path              TEXT,
    swir_path             TEXT,
    false_color_path      TEXT,
    rededge_path          TEXT,
    raw_array_path        TEXT,                 -- NPY or COG
    metadata_json         TEXT,
    content_hash          TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    UNIQUE(tile_id, requested_timestamp, content_hash)
);
CREATE INDEX idx_observations_tile ON observations(tile_id);
CREATE INDEX idx_observations_datetime ON observations(image_datetime);

CREATE TABLE feature_maps (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    observation_id      INTEGER NOT NULL REFERENCES observations(id),
    tile_id             TEXT NOT NULL REFERENCES tiles(id),
    feature_kind        TEXT NOT NULL,         -- 'index:ndvi', 'delta:ndvi', 'z:ndvi'
    formula             TEXT,
    input_bands_json    TEXT NOT NULL,
    raster_path         TEXT NOT NULL,
    png_path            TEXT,
    min_value           REAL, max_value REAL,
    mean_value          REAL, std_value REAL,
    p10_value           REAL, p50_value REAL, p90_value REAL,
    no_data_fraction    REAL,
    content_hash        TEXT NOT NULL,
    created_at          TEXT NOT NULL
);
CREATE INDEX idx_feature_maps_obs ON feature_maps(observation_id);

CREATE TABLE embeddings (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    observation_id  INTEGER NOT NULL REFERENCES observations(id),
    model_name      TEXT NOT NULL,
    model_version   TEXT NOT NULL,
    embedding_dim   INTEGER NOT NULL,
    vector_path     TEXT NOT NULL,             -- NPY
    content_hash    TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    UNIQUE(observation_id, model_name, model_version)
);

CREATE TABLE change_events (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    tile_id                  TEXT NOT NULL REFERENCES tiles(id),
    current_observation_id   INTEGER NOT NULL REFERENCES observations(id),
    baseline_observation_id  INTEGER NOT NULL REFERENCES observations(id),
    candidate_mask_path      TEXT,
    component_count          INTEGER NOT NULL,
    total_area_ha            REAL NOT NULL,
    mean_delta_ndvi          REAL,
    mean_delta_nbr           REAL,
    mean_delta_ndmi          REAL,
    mean_delta_bsi           REAL,
    forest_baseline_fraction REAL,
    quality_score            REAL NOT NULL,
    loss_score               REAL NOT NULL,
    disturbance_score        REAL NOT NULL,
    embedding_anomaly_score  REAL,
    created_at               TEXT NOT NULL
);
CREATE INDEX idx_change_events_tile ON change_events(tile_id);

CREATE TABLE patches (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    change_event_id             INTEGER NOT NULL REFERENCES change_events(id),
    component_index             INTEGER NOT NULL,
    area_ha                     REAL NOT NULL,
    pixel_count                 INTEGER NOT NULL,
    centroid_lon                REAL NOT NULL,
    centroid_lat                REAL NOT NULL,
    bbox_json                   TEXT NOT NULL,
    geometry_geojson            TEXT NOT NULL,
    pixel_geometry_json         TEXT NOT NULL,
    perimeter_m                 REAL,
    compactness                 REAL,
    elongation                  REAL,
    solidity                    REAL,
    edge_sharpness              REAL,
    orientation_deg             REAL,
    statistics_json             TEXT NOT NULL,
    classifier_type             TEXT,           -- DisturbanceType
    classifier_confidence       REAL,
    classifier_probabilities_json TEXT,
    created_at                  TEXT NOT NULL
);
CREATE INDEX idx_patches_event ON patches(change_event_id);

CREATE TABLE alerts (
    id                              TEXT PRIMARY KEY,
    tile_id                         TEXT NOT NULL REFERENCES tiles(id),
    change_event_id                 INTEGER NOT NULL REFERENCES change_events(id),
    primary_patch_id                INTEGER NOT NULL REFERENCES patches(id),
    all_patch_ids_json              TEXT NOT NULL,
    level                           TEXT NOT NULL CHECK (level IN
                                    ('none','watch','investigate','enforcement_review')),
    status                          TEXT NOT NULL,
    disturbance_type                TEXT NOT NULL,
    possible_unauthorized_activity  INTEGER NOT NULL,
    area_estimate_ha                REAL,
    confidence                      REAL,
    deterministic_score             REAL NOT NULL,
    vlm_concurrence                 TEXT,
    legal_context_json              TEXT NOT NULL,
    false_positive_risks_json       TEXT NOT NULL,
    recommended_action              TEXT NOT NULL,
    detection_count                 INTEGER NOT NULL DEFAULT 1,
    evidence_packet_path            TEXT,
    provenance_json                 TEXT NOT NULL,
    created_at                      TEXT NOT NULL,
    updated_at                      TEXT NOT NULL
);
CREATE INDEX idx_alerts_status ON alerts(status);
CREATE INDEX idx_alerts_level ON alerts(level);
CREATE INDEX idx_alerts_tile ON alerts(tile_id);
CREATE INDEX idx_alerts_created ON alerts(created_at);

CREATE TABLE vlm_reports (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    alert_id            TEXT NOT NULL REFERENCES alerts(id),
    role                INTEGER NOT NULL,       -- VlmRole as integer
    model_name          TEXT NOT NULL,
    model_quant         TEXT,
    model_version       TEXT NOT NULL,
    prompt_hash         TEXT NOT NULL,
    input_image_hashes_json TEXT NOT NULL,
    response_json       TEXT NOT NULL,
    schema_valid        INTEGER NOT NULL,
    schema_errors_json  TEXT NOT NULL,
    temperature         REAL NOT NULL,
    max_tokens          INTEGER NOT NULL,
    seed                INTEGER,
    latency_ms          INTEGER NOT NULL,
    created_at          TEXT NOT NULL
);
CREATE INDEX idx_vlm_reports_alert ON vlm_reports(alert_id);
CREATE INDEX idx_vlm_reports_role ON vlm_reports(role);

CREATE TABLE reviews (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    alert_id                    TEXT NOT NULL REFERENCES alerts(id),
    reviewer_id                 TEXT NOT NULL,
    reviewer_role               TEXT,
    review_decision             TEXT NOT NULL,
    review_notes                TEXT,
    field_verification_required INTEGER,
    authority_case_id           TEXT,
    created_at                  TEXT NOT NULL
);
CREATE INDEX idx_reviews_alert ON reviews(alert_id);

CREATE TABLE runs (
    id                  TEXT PRIMARY KEY,
    profile             TEXT NOT NULL,
    code_sha            TEXT NOT NULL,
    config_hash         TEXT NOT NULL,
    started_at          TEXT NOT NULL,
    finished_at         TEXT,
    outcome             TEXT,
    tiles_processed     INTEGER,
    alerts_raised       INTEGER,
    error_message       TEXT
);

CREATE TABLE audit_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id          TEXT,                       -- nullable for system events
    entity_type     TEXT NOT NULL,
    entity_id       TEXT NOT NULL,
    action          TEXT NOT NULL,
    actor           TEXT NOT NULL,              -- 'system' | reviewer_id
    details_json    TEXT,
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_audit_log_entity ON audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_log_run ON audit_log(run_id);
CREATE INDEX idx_audit_log_created ON audit_log(created_at);
```

### Migration Numbering Convention

`NNNN_short_description.sql`. Migrations are append-only; never edit a committed migration. Schema changes happen in new migration files.

---

## 10. JSON Schemas

JSON Schemas live in `schemas/` and are the canonical contracts for VLM outputs and persisted JSON columns. Rust types implement `JsonSchema` (via `schemars`); a CI check (`cargo xtask schema-check`) verifies that `schemars`-generated schemas match the canonical files.

### 10.1 VLM Role 1 — Triage (`schemas/vlm_role_01_triage.schema.json`)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "VlmRole1Triage",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "visible_clouds_over_30_pct": { "type": "boolean" },
    "visible_cloud_shadows": { "type": "boolean" },
    "visible_smoke_or_haze": { "type": "boolean" },
    "geometric_agriculture_pattern": { "type": "boolean" },
    "open_water_or_flooding": { "type": "boolean" },
    "recent_bare_ground_visible": { "type": "boolean" },
    "image_quality": { "type": "string", "enum": ["good","fair","poor","unusable"] },
    "concurrence": { "type": "string", "enum": ["concur","concur_with_caveat","neutral","disagree","unable"] },
    "false_positive_risks": {
      "type": "array",
      "items": { "type": "string",
        "enum": ["cloud","shadow","water","seasonal_crop_harvest","fire","storm_damage",
                 "sensor_or_tile_artifact","temporal_mismatch","georegistration_mismatch","unknown"] }
    },
    "notes": { "type": "string", "maxLength": 500 }
  },
  "required": ["visible_clouds_over_30_pct","visible_cloud_shadows","visible_smoke_or_haze",
               "geometric_agriculture_pattern","open_water_or_flooding","recent_bare_ground_visible",
               "image_quality","concurrence","false_positive_risks","notes"]
}
```

### 10.2 VLM Role 2 — Classification Confirmation

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "VlmRole2ClassificationConfirmation",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "rule_based_classification_shown": { "type": "string" },
    "visual_assessment": {
      "type": "string",
      "enum": ["agree","disagree_burn","disagree_flood","disagree_agriculture",
               "disagree_unclear","agree_with_caveat"]
    },
    "alternative_type": {
      "type": "string",
      "enum": ["clear_cut","selective_logging","logging_road","burn_scar",
               "flood_or_water_change","agriculture_or_harvest","mining","windthrow",
               "storm_damage","cloud_shadow_artifact","unknown_disturbance","none"]
    },
    "confidence_in_visual": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "evidence": { "type": "array", "items": { "type": "string" }, "maxItems": 10 },
    "notes": { "type": "string", "maxLength": 500 }
  },
  "required": ["rule_based_classification_shown","visual_assessment","alternative_type",
               "confidence_in_visual","evidence","notes"]
}
```

### 10.3 VLM Role 3 — Narrative

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "VlmRole3Narrative",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "what_was_detected":   { "type": "string", "maxLength": 1500 },
    "what_visual_evidence_shows": { "type": "string", "maxLength": 1500 },
    "legal_context_summary":{ "type": "string", "maxLength": 1500 },
    "recommended_action_explanation": { "type": "string", "maxLength": 1500 },
    "evidence_citations":  { "type": "array", "items": { "type": "string" }, "maxItems": 10 },
    "legal_disclaimer":    { "type": "string", "const":
      "This system does not make legal accusations. Findings require authority review and possibly ground verification." }
  },
  "required": ["what_was_detected","what_visual_evidence_shows","legal_context_summary",
               "recommended_action_explanation","evidence_citations","legal_disclaimer"]
}
```

### 10.4 VLM Roles 4–7

Schemas for roles 4 (map reading), 5 (reviewer chat), 6 (ground photo cross-check), and 7 (daily summary) follow the same flat-object pattern. Full text in `schemas/`.

### 10.5 Alert Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Alert",
  "type": "object",
  "properties": {
    "id":                              { "type": "string", "format": "uuid" },
    "tile_id":                         { "type": "string" },
    "change_event_id":                 { "type": "integer" },
    "primary_patch_id":                { "type": "integer" },
    "level":                           { "enum": ["none","watch","investigate","enforcement_review"] },
    "status":                          { "type": "string" },
    "disturbance_type":                { "type": "string" },
    "possible_unauthorized_activity":  { "type": "boolean" },
    "area_estimate_ha":                { "type": "number" },
    "confidence":                      { "type": "number", "minimum": 0, "maximum": 1 },
    "deterministic_score":             { "type": "number", "minimum": 0, "maximum": 1 },
    "vlm_concurrence":                 { "type": ["string","null"] },
    "legal_context":                   { "$ref": "#/definitions/LegalContext" },
    "false_positive_risks":            { "type": "array", "items": { "type": "string" } },
    "recommended_action":              { "type": "string" },
    "detection_count":                 { "type": "integer", "minimum": 1 },
    "evidence_packet_path":            { "type": ["string","null"] },
    "provenance":                      { "$ref": "#/definitions/Provenance" },
    "created_at":                      { "type": "string", "format": "date-time" },
    "updated_at":                      { "type": "string", "format": "date-time" }
  },
  "required": ["id","tile_id","level","status","disturbance_type",
               "possible_unauthorized_activity","deterministic_score",
               "legal_context","recommended_action","provenance","created_at","updated_at"],
  "definitions": {
    "LegalContext": { /* per §8.9 */ },
    "Provenance":   { /* per §8.8 */ }
  }
}
```

### 10.6 Evidence Manifest Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "EvidenceManifest",
  "type": "object",
  "properties": {
    "alert_id":           { "type": "string" },
    "tile_id":            { "type": "string" },
    "created_at":         { "type": "string", "format": "date-time" },
    "system_version":     { "type": "string" },
    "code_sha":           { "type": "string" },
    "model_refs":         { "type": "array", "items": { "$ref": "#/definitions/ModelRef" } },
    "current_observation":  { "$ref": "#/definitions/ObservationSummary" },
    "baseline_observation": { "$ref": "#/definitions/ObservationSummary" },
    "disturbance_summary":  { "$ref": "#/definitions/DisturbanceSummary" },
    "legal_context":        { "$ref": "#/definitions/LegalContext" },
    "files":              {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "properties": {
          "path":   { "type": "string" },
          "sha256": { "type": "string", "pattern": "^[a-f0-9]{64}$" },
          "bytes":  { "type": "integer" }
        },
        "required": ["path","sha256","bytes"]
      }
    },
    "manifest_signature": {
      "type": "object",
      "properties": {
        "algorithm":   { "type": "string", "enum": ["ed25519"] },
        "public_key":  { "type": "string" },
        "signature":   { "type": "string" }
      }
    }
  },
  "required": ["alert_id","tile_id","created_at","system_version","code_sha",
               "model_refs","current_observation","baseline_observation",
               "disturbance_summary","legal_context","files"]
}
```

---

## 11. HTTP API Contract

`fg-server` exposes both an HTML dashboard and a JSON API. JSON routes prefixed `/api/v1`.

### 11.1 Routes

```
GET  /                                  HTML dashboard home (alert queue)
GET  /tiles                             HTML map of tiles + AOIs
GET  /alerts                            HTML alert queue (filterable)
GET  /alerts/{id}                       HTML alert detail (evidence view)
POST /alerts/{id}/review                form submission, htmx
GET  /events                            Server-Sent Events stream

GET  /api/v1/health                     200 OK if all dependencies reachable
GET  /api/v1/aois                       List AOIs
GET  /api/v1/aois/{id}                  AOI detail with tiles
GET  /api/v1/tiles                      List tiles (?aoi_id=&active=)
GET  /api/v1/tiles/{id}                 Tile detail with observations
GET  /api/v1/observations/{id}          Observation detail
GET  /api/v1/change_events/{id}         ChangeEvent detail with patches
GET  /api/v1/alerts                     List alerts (filters: status, level, since)
GET  /api/v1/alerts/{id}                Alert detail
POST /api/v1/alerts/{id}/review         Submit review
GET  /api/v1/alerts/{id}/evidence       Manifest JSON
GET  /api/v1/alerts/{id}/evidence/files/{filename}   Evidence file download
GET  /api/v1/runs                       List runs
GET  /api/v1/runs/{id}                  Run detail with audit entries
POST /api/v1/runs                       Trigger a run (admin only)

POST /api/v1/vlm/chat                   Reviewer chat (Role 5) — alert_id, message
POST /api/v1/vlm/explain/{alert_id}     Trigger Role 3 narrative regeneration
POST /api/v1/vlm/ground_photo           Multipart: alert_id + image (Role 6)

GET  /api/v1/reports/daily?date=        Daily summary (Role 7)
```

### 11.2 Authentication (MVP)

MVP: HTTP basic auth on POST routes (single reviewer credential). Production: OIDC.

### 11.3 Error Responses

All non-2xx responses return:

```json
{
  "error": {
    "category": "validation | transient | permanent | internal",
    "code": "FG_E_XXXX",
    "message": "human-readable",
    "details": { /* optional */ }
  }
}
```

---

## 12. SimSat Client Specification

### 12.1 Base URL

Default `http://localhost:9005`. Configurable via `simsat.base_url`.

### 12.2 Endpoints Used

| Endpoint | Pipeline use |
|---|---|
| `GET /data/current/position` | live demo ticker; never for evidence |
| `GET /data/current/image/sentinel` | live demo only |
| `GET /data/image/sentinel` | **primary endpoint** for monitoring |

### 12.3 `get_image_sentinel` parameters

```
lon: f64
lat: f64
timestamp: ISO 8601 UTC
spectral_bands: comma-separated (e.g. "red,green,blue,nir,swir16,swir22")
size_km: f64
return_type: "png" | "array"
window_seconds: u64 (default 864000 = 10 days)
```

### 12.4 Ingest Rules (apply at the boundary, never deeper)

1. Cast all numeric arrays to `f32`.
2. Normalize to `[0, 1]` reflectance (apply scale_factor based on `source_dtype`).
3. Build `BandStack` with explicit `bands` axis order.
4. Compute `content_hash = SHA256(bytes(band_data) || band_names || footprint_json)`.
5. Persist to `observations` table with the hash; if a row with the same hash exists, do not duplicate.

---

## 13. Spectral Indices and Masks

### 13.1 Index Formulas (canonical)

All formulas use `safe_div(num, den, eps)` with `eps = 1e-6` and operate elementwise on `f32` arrays clamped to `[0, 1]` reflectance.

```
NDVI = (NIR  - Red)   / (NIR  + Red)
NDMI = (NIR  - SWIR1) / (NIR  + SWIR1)
NBR  = (NIR  - SWIR2) / (NIR  + SWIR2)
BSI  = ((SWIR1 + Red) - (NIR + Blue)) / ((SWIR1 + Red) + (NIR + Blue))
NDWI = (Green - NIR)  / (Green + NIR)
NDRE = (NIR - RedEdge2) / (NIR + RedEdge2)
```

After computation, all index maps are clamped to `[-1, 1]` (BSI and NDRE included).

### 13.2 Forest Baseline Mask (humid tropical default)

```
forest_baseline =
    NDVI_baseline > 0.55
  & NDMI_baseline > 0.10
  & NDWI_baseline < 0.30
  & BSI_baseline  < 0.20
```

For other biomes, use `fg_detect::thresholds::for_biome(biome)`. Documented strata: `humid_tropical`, `dry_forest`, `mangrove`, `plantation`. The MVP ships with these; real-world deployment requires per-region calibration.

### 13.3 Water Mask

```
water = NDWI > 0.35 & NDVI < 0.30
```

### 13.4 Cloud Mask

MVP: from SimSat metadata `cloud_cover` (whole-scene). Documented next-step: per-pixel from `s2cloudless`-equivalent CNN.

### 13.5 Candidate Loss Mask

```
candidate_loss =
    forest_baseline
  & ΔNDVI < -0.25
  & ΔNBR  < -0.25
  & ΔBSI  > +0.15
  & NDVI_current < 0.45
  & ¬water
  & ¬cloud
```

Adaptive (preferred when seasonal time-series available):

```
candidate_loss =
    forest_baseline
  & z_NDVI < -3
  & z_NBR  < -3
  & z_BSI  > +3
  & ¬water
  & ¬cloud
```

`z_X = (X_current - seasonal_median_X) / max(seasonal_MAD_X, eps)`.

---

## 14. Change Detection and Patch Extraction

### 14.1 Connected Components

Implementation: 8-connectivity, integer label image, in `fg_detect::patches`. Backend: `imageproc::region_labelling::connected_components` or hand-rolled (validated against scipy.ndimage.label on test fixtures).

### 14.2 Patch Filtering

After labeling:

1. Drop components with `pixel_count < 25` (= 0.25 ha at 10 m). Stored as "sub-MMU" only if logged for future accumulation.
2. Apply morphological opening (3×3) before labeling to suppress single-pixel noise.
3. For each retained component, compute `PatchShape` and `PatchPixelStatistics`.

### 14.3 Patch-to-Geometry

For each patch:
1. Extract pixel-space polygon (boundary of the connected component).
2. Apply the observation's `AffineTransform` to convert to lon/lat.
3. Simplify (Douglas-Peucker, tolerance 1 m) to reduce polygon size.
4. Store both pixel-space and lon/lat polygons (per ADR-0019).

### 14.4 ChangeEvent Composition

A ChangeEvent is the unit of "one comparison." It contains zero or more Patches and aggregate statistics. A Tile pair (T_current, T_baseline) produces exactly one ChangeEvent.

---

## 15. Deterministic Scoring

### 15.1 Loss Score

```
loss_score = clamp01(
    0.25 * norm_neg(mean_delta_ndvi, threshold = -0.25) +
    0.25 * norm_neg(mean_delta_nbr,  threshold = -0.25) +
    0.15 * norm_pos(mean_delta_bsi,  threshold = +0.15) +
    0.10 * forest_baseline_fraction +
    0.10 * patch_area_score(area_ha) +
    0.10 * persistence_score(detection_count) +
    0.05 * legal_context_score(legal_context)
)
```

Where:

```
norm_neg(x, t) = clamp01(-x / max(-t, eps)) for x <= 0; else 0
norm_pos(x, t) = clamp01( x / max( t, eps)) for x >= 0; else 0
patch_area_score(a) = clamp01(log10(max(a, 0.01) + 1) / 2)         // 0.01 ha → 0.0; 100 ha → 1.0
persistence_score(n) = clamp01((n - 1) / 4)                         // 1 → 0.0; 5+ → 1.0
legal_context_score(lc) =
    0.5 * (lc.inside_protected_area as f32) +
    0.3 * (!lc.overlaps_known_permit as f32) +
    0.2 * (lc.inside_indigenous_or_community_land as f32)
```

### 15.2 Quality Score

```
quality_score = clamp01(
    1.0
    - 0.5 * cloud_cover_fraction
    - 0.3 * no_data_fraction
    - 0.2 * misregistration_risk
)
```

`misregistration_risk` is derived from the time gap between observations and the source satellite identities (different satellites → small risk addition).

### 15.3 Disturbance Score

```
disturbance_score = loss_score * quality_score
```

### 15.4 Alert Level Mapping

| `disturbance_score` | Level (preliminary) |
|---|---|
| `[0.00, 0.30)` | `none` |
| `[0.30, 0.55)` | `watch` |
| `[0.55, 0.75)` | `investigate` |
| `[0.75, 1.00]` | `enforcement_review` (requires gates) |

**`enforcement_review` requires all of:**

- `cloud_cover ≤ 20%`
- `no_data_fraction ≤ 10%`
- `area_ha ≥ 0.10` (note: alert generation requires 0.25 ha; this is a redundant guard)
- `forest_baseline_fraction ≥ 0.60`
- `detection_count ≥ 1` (will be `≥ 2` when multi-observation confirmation is enabled)
- VLM Role 1 did not flag `image_quality = unusable` or strong cloud/water/agriculture risks

If any gate fails, the level is downgraded one tier.

---

## 16. Legal Context Engine

### 16.1 Inputs

GeoJSON files in `data/aois/`, loaded at server start:

| File | Type |
|---|---|
| `protected_areas.geojson` | WDPA-derived |
| `forest_reserves.geojson` | national/state |
| `logging_concessions.geojson` | national |
| `valid_permits.geojson` | recent permit polygons + valid dates |
| `indigenous_lands.geojson` | RAISG / national equivalents |
| `community_forests.geojson` | local |
| `known_agriculture.geojson` | OSM landuse=farmland or national crop maps |
| `roads.geojson` | OSM highway=* (filtered by class) |
| `settlements.geojson` | OSM place=* (city, town, village) |

Each feature must have `id`, `name`, `source`, `valid_from`, `valid_to` (nullable).

### 16.2 Spatial Index

R-tree per layer, built at load. Queries return candidate IDs in sub-millisecond.

### 16.3 Decision Rules

Given a Patch polygon, evaluate:

```rust
fn evaluate(patch: &Polygon, layers: &Layers) -> LegalContext {
    let inside_protected = layers.protected.intersects_any(patch);
    let inside_concession = layers.concessions.intersects_any(patch);
    let inside_indigenous = layers.indigenous.intersects_any(patch)
                          || layers.community.intersects_any(patch);
    let overlaps_permit = layers.valid_permits.intersects_any_within_dates(patch, today());
    let dist_road = layers.roads.distance_meters(patch.centroid());
    let dist_settlement = layers.settlements.distance_meters(patch.centroid());

    let possible_unauthorized = match (inside_protected, overlaps_permit, inside_concession) {
        (true,  false, _) => true,
        (false, false, true) => true,   // concession but no permit polygon
        _ => false,
    };

    let legal_conclusion = if possible_unauthorized {
        LegalConclusion::RequiresAuthorityReview
    } else {
        LegalConclusion::NotALegalConclusion
    };

    LegalContext {
        inside_protected_area: inside_protected,
        overlaps_known_permit: overlaps_permit,
        inside_logging_concession: inside_concession,
        inside_indigenous_or_community_land: inside_indigenous,
        distance_to_known_road_m: Some(dist_road),
        distance_to_settlement_m: Some(dist_settlement),
        possible_unauthorized_activity: possible_unauthorized,
        legal_conclusion,
        matched_layer_ids: collect_matched_ids(),
    }
}
```

### 16.4 Important Constraints

- Permit overlap **never** sets `possible_unauthorized_activity = false` automatically; it must overlap **and** be valid at `image_datetime`.
- Indigenous/community-land overlap **never** by itself flips `possible_unauthorized_activity`. The handling is jurisdiction-specific and goes to authority review either way.
- The engine never produces `possible_unauthorized_activity = true` for a patch that overlaps a known agriculture polygon without also being in a protected area.

---

## 17. Foundation Model Embeddings

### 17.1 Model Choice

**Default:** Clay v1 (multispectral foundation model, MAE-based, supports flexible band/resolution input). **Alternate:** Prithvi-EO-2.0 (NASA-IBM, 300M / 600M, HLS-trained).

### 17.2 ONNX Export (in `tools/export_foundation_model.py`)

```python
# pseudo-code; full file in tools/
import torch, onnx
from clay import ClayMAE

model = ClayMAE.from_pretrained("clay-v1")
model.eval()

dummy = torch.zeros(1, 6, 256, 256)            # (B, bands, H, W)
torch.onnx.export(
    model.encoder,
    (dummy,),
    "models/clay_v1_encoder.onnx",
    input_names=["bands"],
    output_names=["embedding"],
    dynamic_axes={"bands": {0: "B"}, "embedding": {0: "B"}},
    opset_version=17,
)
```

### 17.3 Rust Inference (`fg-embed`)

```rust
pub struct ClayV1 {
    session: ort::Session,
    band_order: Vec<BandName>,                  // [Blue, Green, Red, Nir, Swir16, Swir22]
    input_resolution: u32,                      // 256
    embedding_dim: u32,                         // model-dependent, e.g. 768
}

impl FoundationEmbedder for ClayV1 {
    async fn embed(&self, stack: &BandStack) -> Result<Embedding, EmbedError> {
        let resampled = resample_to(stack, self.input_resolution)?;
        let aligned = reorder_bands(resampled, &self.band_order)?;
        let input = ndarray_to_ort_tensor(aligned)?;
        let outputs = self.session.run([input.into()])?;
        let emb = outputs[0].try_extract_tensor::<f32>()?;
        Ok(Embedding { dim: self.embedding_dim, vector: emb.to_owned().into_raw_vec() })
    }
}
```

### 17.4 Anomaly Score

```
embedding_anomaly_score(current, baseline) = 1 - cosine_similarity(current, baseline)
```

Range `[0, 2]`; values `> 0.3` are typical anomalies, `> 0.6` strong.

The score enters the patch feature vector for the LightGBM classifier (§20) and is exposed as a separate Optional field on `Patch`.

---

## 18. The Seven VLM Roles

Each role: **Purpose / Trigger / Input / Output schema / Asymmetric judgment rule / Latency budget**.

### 18.1 Role 1: Triage (RGB sanity check)

- **Purpose:** Cheap, fast false-positive filter using only RGB.
- **Trigger:** Every Alert at level `watch` or higher.
- **Input:**
  - 1 image: RGB before/after side-by-side, 512×256 px.
  - Text: numeric feature summary (mean ΔNDVI, ΔNBR, ΔBSI, area_ha, cloud_cover, days_between_obs).
- **Output schema:** `vlm_role_01_triage.schema.json` (§10.1).
- **Asymmetric judgment rule:**
  - If `image_quality = unusable` → set Alert status `awaiting_second_observation`, no level change.
  - If `visible_clouds_over_30_pct = true` → demote level by one tier; add `Cloud` to false-positive risks.
  - If `geometric_agriculture_pattern = true` AND `disturbance_type = clear_cut` → demote to `watch`; add `SeasonalCropHarvest` risk.
  - If `open_water_or_flooding = true` AND area was not previously water → demote; add `Water` risk.
  - **Cannot promote.**
- **Latency budget:** 800 ms median, 2 s P95.

### 18.2 Role 2: Classification Confirmation

- **Purpose:** Visually confirm or contest the rule-based / classifier disturbance type.
- **Trigger:** Every Alert at level `investigate` or higher.
- **Input:**
  - 1 image: RGB before/after side-by-side, 512×256.
  - Text: `rule_based_classification`, `classifier_top_1`, `classifier_top_2`.
- **Output schema:** §10.2.
- **Asymmetric judgment rule:**
  - If `visual_assessment = agree` or `agree_with_caveat` → keep type.
  - If `disagree_*` → set `disturbance_type` to the alternative; if alternative is `cloud_shadow_artifact` or `flood_or_water_change`, demote level by one tier.
  - VLM cannot upgrade `selective_logging` to `clear_cut`. It can downgrade `clear_cut` to a milder type.
- **Latency budget:** 1.5 s median.

### 18.3 Role 3: Narrative

- **Purpose:** Generate the human-readable evidence-packet narrative.
- **Trigger:** Every Alert at `investigate` or higher, after Roles 1 and 2.
- **Input:**
  - 1 image: RGB current.
  - Text: full Alert JSON (sanitized).
- **Output schema:** §10.3 (four paragraph fields + citations + mandatory legal disclaimer).
- **Asymmetric judgment rule:** None. Narrative is purely descriptive; if it disagrees with structured fields it is logged as a schema violation and the structured fields win.
- **Latency budget:** 3 s median.

### 18.4 Role 4: Map Reading

- **Purpose:** Answer reviewer questions about a rendered alert map.
- **Trigger:** On reviewer request from the dashboard.
- **Input:**
  - 1 image: rendered map (basemap + alert geometry + legal-overlay polygons + scale bar + legend).
  - Text: reviewer's question.
- **Output schema:** flat object with `answer`, `confidence`, `caveats`, `unable_to_determine`.
- **Asymmetric judgment rule:** Never affects the alert.

### 18.5 Role 5: Reviewer Chat

- **Purpose:** Conversational explanation over the Alert JSON.
- **Trigger:** Reviewer types in dashboard chat.
- **Input:**
  - 1 image: RGB current chip.
  - Text: full Alert JSON + conversation history (turn-truncated).
- **Output schema:** `{ "answer": string, "cited_fields": string[] }`.
- **Asymmetric judgment rule:** Never affects the alert. Logged as VlmReport.

### 18.6 Role 6: Ground-Photo Cross-Check

- **Purpose:** Compare a field-team ground photo to the satellite-derived expectation.
- **Trigger:** Field team uploads a photo via API.
- **Input:**
  - 2 images: RGB satellite chip + ground photo.
  - Text: expected disturbance type + location.
- **Output schema:** `{ "ground_consistent_with_satellite": boolean, "discrepancies": string[], "confidence": number, "notes": string }`.
- **Asymmetric judgment rule:** If `ground_consistent_with_satellite = false`, set Alert status to `human_reviewed` and add a flag in evidence packet.

### 18.7 Role 7: Daily Summary

- **Purpose:** End-of-day stakeholder summary.
- **Trigger:** Cron job at 23:55 UTC.
- **Input:**
  - 0 images.
  - Text: aggregated alert counts, top false-positive patterns, AOIs with new alerts, summary statistics.
- **Output schema:** `{ "summary_paragraphs": string[], "top_aois": string[], "common_fp_patterns": string[] }`.
- **Asymmetric judgment rule:** None. Stored in `daily_summaries` and surfaced in dashboard.

---

## 19. VLM Backend (llama-server) Integration

### 19.1 Server Launch

```bash
llama-server \
  -hf LiquidAI/LFM2-VL-1.6B-GGUF:Q8_0 \
  --jinja \
  --port 8080 \
  --temp 0.2 \
  --top-p 0.9 \
  --n-predict 800 \
  --seed 42
```

`--json-schema` is set per-request via the OpenAI-compatible `response_format` field.

### 19.2 Rust Client (`fg_vlm::VlmClient`)

```rust
pub async fn run_role(&self, role: VlmRole, input: VlmInput) -> Result<VlmReport, VlmError> {
    let body = OpenAIChatRequest {
        model: "lfm2-vl",
        messages: vec![
            Message::system(input.system_prompt),
            Message::user_with_images(input.user_text, input.images),
        ],
        response_format: Some(ResponseFormat::JsonSchema { schema: input.schema.clone() }),
        temperature: input.temperature,
        max_tokens: input.max_tokens,
        seed: Some(self.config.seed),
    };

    let started = Instant::now();
    let raw = self.http.post(&self.endpoint).json(&body).send().await?.text().await?;
    let latency = started.elapsed();

    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
    let content_str = extract_content(&parsed)?;
    let content_json: serde_json::Value = serde_json::from_str(&content_str)?;

    let validator = jsonschema::JSONSchema::compile(&input.schema)?;
    let valid = validator.is_valid(&content_json);
    let errors = if !valid {
        validator.validate(&content_json).err()
            .map(|errs| errs.map(|e| e.to_string()).collect())
            .unwrap_or_default()
    } else { vec![] };

    Ok(VlmReport {
        id: VlmReportId(0),                    // assigned on insert
        alert_id: AlertId(Uuid::nil()),        // filled by caller
        role,
        model: self.model_ref.clone(),
        prompt_hash: hash_prompt(&input.system_prompt, &input.user_text),
        input_image_hashes: hash_images(&input.images),
        response_json: content_json,
        schema_valid: valid,
        schema_errors: errors,
        temperature: input.temperature,
        max_tokens: input.max_tokens,
        seed: Some(self.config.seed),
        latency_ms: latency.as_millis() as u64,
        created_at: Utc::now(),
    })
}
```

### 19.3 Retry Policy

- On HTTP 5xx or timeout: retry once with same prompt.
- On schema-invalid output: retry once with stricter system prompt suffix `Return only valid JSON. Do not include any explanation or commentary outside the JSON.`
- On second failure: persist the report with `schema_valid = false` and proceed; the alert defaults to deterministic findings only.

### 19.4 Determinism

- `seed` is set explicitly per role (in config).
- `temperature` defaults to 0.2 for production runs, 0.0 for tests.
- The same input must produce the same output; the test suite asserts this on fixtures.

### 19.5 Image Pre-Processing

- Resize to model native input (LFM2-VL: 512×512 or compatible). LFM2.5-VL-450M: 512×512.
- Convert to JPEG with quality 92 before base64-encoding into the API call (smaller payloads, llama-server expects standard image input).
- Hash the **post-resize, post-JPEG** bytes for `input_image_hashes` so re-runs are reproducible.

---

## 20. LightGBM Classifier

### 20.1 Purpose

Predict `DisturbanceType` from the per-Patch feature vector. Faster, more accurate, and more reliable on multispectral signals than the VLM.

### 20.2 Feature Vector

```rust
pub struct PatchFeatureVector {
    // Index statistics
    pub mean_ndvi_current: f32,
    pub mean_ndvi_baseline: f32,
    pub mean_delta_ndvi: f32,
    pub std_delta_ndvi: f32,
    pub mean_nbr_current: f32,
    pub mean_nbr_baseline: f32,
    pub mean_delta_nbr: f32,
    pub mean_bsi_current: f32,
    pub mean_delta_bsi: f32,
    pub mean_ndmi_current: f32,
    pub mean_delta_ndmi: f32,
    pub mean_ndwi_current: f32,
    pub mean_delta_ndwi: f32,
    // Shape
    pub area_ha: f32,
    pub perimeter_m: f32,
    pub compactness: f32,
    pub elongation: f32,
    pub solidity: f32,
    pub edge_sharpness: f32,
    pub orientation_deg: f32,
    // Context
    pub forest_baseline_fraction: f32,
    pub water_fraction: f32,
    pub cloud_fraction: f32,
    pub days_between_observations: f32,
    pub embedding_anomaly_score: f32,           // 0 if unavailable
    // Biome (one-hot)
    pub is_humid_tropical: f32,
    pub is_dry_forest: f32,
    pub is_mangrove: f32,
    pub is_plantation: f32,
}
```

Total: 28 features. Stable order; documented in `crates/fg-classify/src/feature_order.rs`.

### 20.3 Training (`tools/train_lightgbm.py`)

```python
import lightgbm as lgb
import pandas as pd

df = pd.read_parquet("data/labels/patches_features.parquet")
X = df[FEATURE_ORDER].values
y = df["disturbance_type"].astype("category").cat.codes

train, valid = stratified_split(df, by="biome,disturbance_type")

params = dict(
    objective="multiclass",
    num_class=NUM_CLASSES,
    metric="multi_logloss",
    learning_rate=0.05,
    num_leaves=31,
    feature_fraction=0.9,
    bagging_fraction=0.8,
    bagging_freq=5,
    min_data_in_leaf=20,
    verbose=-1,
)

model = lgb.train(params, train, num_boost_round=500,
                  valid_sets=[valid], callbacks=[lgb.early_stopping(30)])
model.save_model("models/lgb_disturbance_type.txt")
```

### 20.4 Output Classes

```
0 = clear_cut
1 = selective_logging
2 = logging_road
3 = burn_scar
4 = flood_or_water_change
5 = agriculture_or_harvest
6 = mining
7 = windthrow
8 = cloud_shadow_artifact
9 = unknown_disturbance
```

### 20.5 Asymmetric Integration with VLM Role 2

1. LightGBM produces top-1 + probabilities.
2. Role 2 receives `rule_based_classification = lgb_top_1` and `classifier_top_2 = lgb_top_2`.
3. If Role 2 disagrees, the rule from §18.2 applies.

---

## 21. Evidence Packet Specification

### 21.1 Directory Layout

```
data/alert_packets/ALERT_<uuid>/
├── manifest.json                       # canonical (schema §10.6)
├── current_rgb.png                     # 1024×1024 max, 8-bit
├── baseline_rgb.png
├── current_swir.png                    # SWIR2/SWIR1/Red false-color
├── baseline_swir.png
├── delta_ndvi.png                      # diverging colormap
├── delta_nbr.png
├── delta_bsi.png
├── candidate_loss_mask.png
├── legal_overlay.png                   # alert geometry + WDPA polygon
├── eight_panel_composite.png           # all 8 above as a single image (reviewer view)
├── rgb_before_after.png                # the chip the VLM saw (Role 1, 2)
├── feature_summary.json                # numeric features, classifier output, scoring
├── current_observation_metadata.json
├── baseline_observation_metadata.json
├── alert.json                          # the full Alert aggregate
├── vlm_reports/
│   ├── role_01_triage.json
│   ├── role_02_classification.json
│   └── role_03_narrative.json
├── report.md                           # Role 3 narrative rendered + structured sections
├── hashes.txt                          # `<sha256>  <relative_path>` per file
└── manifest.json.sig                   # ed25519 signature of manifest.json (optional)
```

### 21.2 Hashing

Every file in the packet (except `manifest.json` and `manifest.json.sig`) is SHA-256 hashed; hashes go into `manifest.json` `files` map and into `hashes.txt`. Then `manifest.json` itself is hashed and persisted in the `alerts.evidence_packet_path` foreign-key context.

### 21.3 Signing (optional)

If `evidence.signing_key_path` is configured, the manifest is signed with Ed25519. The public key is stored in `data/keys/public.key` and surfaced in the dashboard for verifiers.

### 21.4 Verification

`fg packet-verify <path>`:
1. Parse `manifest.json`.
2. For each entry in `files`, recompute SHA-256 and compare.
3. If a signature is present, verify against the public key.
4. Report any mismatches.

---

## 22. Audit Log and Provenance

### 22.1 What Goes in the Audit Log

Every state transition of every aggregate. Examples:

| `entity_type` | `action` | when |
|---|---|---|
| `run` | `started` | pipeline run begins |
| `observation` | `fetched` | SimSat returned data |
| `change_event` | `computed` | masks + patches done |
| `alert` | `raised` | alert level ≥ watch |
| `vlm_report` | `created` | each VLM call |
| `alert` | `vlm_demoted` | asymmetric judgment fired |
| `evidence_packet` | `sealed` | all hashes computed |
| `review` | `submitted` | reviewer action |
| `alert` | `status_changed` | any status update |

### 22.2 Provenance per Alert

Stored verbatim in `alerts.provenance_json`:

```json
{
  "run_id": "uuid",
  "code_sha": "abc1234...",
  "config_hash": "sha256:...",
  "current_observation_id": 12,
  "baseline_observation_id": 7,
  "model_refs": [
    { "kind": "Vlm", "name": "LiquidAI/LFM2-VL-1.6B-GGUF", "quant": "Q8_0", "version": "lfm2-vl-1.6b-2025q4", "hash": "sha256:..." },
    { "kind": "Embedder", "name": "Clay v1", "version": "1.0.0", "hash": "sha256:..." },
    { "kind": "Classifier", "name": "lgb_disturbance_type", "version": "0.1.0-synthetic", "hash": "sha256:..." }
  ],
  "thresholds_used": { "...": "as in config" },
  "started_at": "2026-05-05T10:00:00Z",
  "finished_at": "2026-05-05T10:00:14Z"
}
```

### 22.3 `replay.py`

A separate tool (in `tools/replay.py`) takes an `alert_id`, looks up its provenance, re-runs the pipeline with frozen inputs, and asserts the produced alert matches the stored alert byte-for-byte. This is the integrity check.

---

## 23. Dashboard Specification

### 23.1 Pages

**Home (`/`)**
- Map: tiles (small dots), alerts (colored pins by level).
- Side panel: alert queue (filter by status/level), most recent runs, KPI cards (alerts today, alerts pending review, alerts in protected areas).

**Alert detail (`/alerts/{id}`)**
- Top: alert metadata (level, type, score, area, AOI, legal context).
- Eight-panel composite viewer (zoomable).
- Tabs: Evidence | VLM Reports | Provenance | Reviewer Chat (Role 5) | Audit Log.
- Action buttons: Confirm / Dismiss / Request next obs / Send to enforcement / Add note.

**Tile detail (`/tiles/{id}`)**
- Observation timeline.
- Past alerts.
- Manual "fetch now" button (admin).

**Run detail (`/runs/{id}`)**
- Step-by-step trace with timings.
- All audit entries.

### 23.2 Tech

- Server-rendered HTML via `askama`.
- Interactivity via `htmx` (forms post, return HTML fragments).
- Map via MapLibre GL JS, GeoJSON loaded from API.
- SSE (`/events`) for live updates.
- No bundler. CDN for MapLibre, htmx.

### 23.3 Reviewer Workflow

Buttons map to `POST /api/v1/alerts/{id}/review` with form data:

| Button | `decision` | Side effects |
|---|---|---|
| Confirm disturbance | `confirmed_disturbance` | status → `confirmed_disturbance` |
| Dismiss as false positive | `dismissed_false_positive` | status → `dismissed_false_positive` |
| Request next observation | `awaiting_second_observation` | status → `awaiting_second_observation` |
| Send to enforcement | `sent_to_authority` | status → `sent_to_authority`, prompts for `authority_case_id` |
| Add note | (no status change) | appends to `reviews` |

All reviewer actions append to `audit_log`.

---

## 24. Configuration

### 24.1 File: `config/forest-guardian.toml`

```toml
[server]
bind = "127.0.0.1:3000"
secret_key = "${FG_SECRET_KEY}"

[database]
url = "sqlite://forest_guardian.db?mode=rwc"
max_connections = 8

[simsat]
base_url = "http://localhost:9005"
timeout_seconds = 30
default_size_km = 5.0
default_window_seconds = 864000              # 10 days

[vlm]
backend = "llama-server"
base_url = "http://localhost:8080"
model_name = "LiquidAI/LFM2-VL-1.6B-GGUF"
model_quant = "Q8_0"
model_version = "lfm2-vl-1.6b-2025q4"
temperature = 0.2
top_p = 0.9
max_tokens = 800
seed = 42
timeout_seconds = 60
roles_enabled = [1, 2, 3]                   # MVP

[embed]
model = "clay-v1"
onnx_path = "models/clay_v1_encoder.onnx"
device = "cpu"
input_resolution = 256

[classifier]
enabled = false                              # MVP starts disabled; enabled after first training
model_path = "models/lgb_disturbance_type.txt"

[legal]
data_dir = "data/aois"

[evidence]
output_dir = "data/alert_packets"
signing_key_path = ""                       # empty disables signing

[thresholds.humid_tropical]
ndvi_baseline_min = 0.55
ndmi_baseline_min = 0.10
ndwi_baseline_max = 0.30
bsi_baseline_max = 0.20
delta_ndvi_max = -0.25
delta_nbr_max = -0.25
delta_bsi_min = 0.15
ndvi_current_max = 0.45
min_pixel_count = 25                         # 0.25 ha at 10m
adaptive_z_threshold = 3.0
cloud_cover_warning = 0.20
cloud_cover_block = 0.40

[thresholds.dry_forest]
# documented per-biome overrides
# ...

[scoring]
# weights as in §15.1
loss_weight_ndvi = 0.25
loss_weight_nbr  = 0.25
loss_weight_bsi  = 0.15
loss_weight_forest_fraction = 0.10
loss_weight_area = 0.10
loss_weight_persistence = 0.10
loss_weight_legal = 0.05
quality_weight_cloud = 0.50
quality_weight_nodata = 0.30
quality_weight_misregistration = 0.20

[telemetry]
log_level = "info"
log_format = "json"

[runs]
default_profile = "demo"
parallelism = 4
```

### 24.2 Environment Overrides

Any TOML key can be overridden by `FG_<UPPER_SNAKE_CASE_PATH>`. Example: `FG_VLM__BASE_URL=http://gpu-host:8080`.

### 24.3 Config Hash

At run time, the loaded, fully-resolved config is serialized to canonical JSON and SHA-256 hashed. The hash is stored in `Provenance.config_hash`.

---

## 25. Fine-Tuning Plan

The MVP runs zero-shot. Fine-tuning is a phased deliverable. This section specifies, for each role, the **training-data shape** in detail sufficient to begin collection without further design.

### 25.1 Common Infrastructure

- Format: JSONL with one example per line.
- Each example has: `image_paths` (list), `system_prompt`, `user_text`, `expected_output_json`, `metadata` (provenance, biome, source).
- Storage: `data/labels/finetune/role_NN/`.
- Adapter type: LoRA. Rank 16. Alpha 32. Targets: q_proj, k_proj, v_proj, o_proj.
- Training tool: Hugging Face TRL or LiquidAI's recommended pipeline (`LEAP` or equivalent).
- Output: GGUF-merged adapters compatible with llama-server.

### 25.2 Role 1 — Triage

**Goal:** Produce reliable binary answers about visible RGB phenomena.

**Dataset size:** 5,000–10,000 examples.

**Composition:**

| Source | Count | Description |
|---|---|---|
| SimSat-augmented | 3,000 | RGB before/after chips with synthetic cloud, shadow, water, agriculture, smoke overlays added programmatically. Labels auto-generated from overlay parameters. |
| Real Sentinel-2 RGB | 1,500 | From Earth Search STAC. Hand-labeled triage answers. Stratified across biomes. |
| Negative controls | 500 | Stable forest with no overlays. Labels: all `false`, quality `good`, concur `concur`. |
| Ambiguous edges | 500 | Hand-curated tricky cases; multiple annotators; majority label. |

**Augmentation pipeline (`tools/build_finetune_dataset.py role_01`):**

```python
def synth_cloud_overlay(rgb, cloud_fraction):
    # alpha-blend white-noise patches with Gaussian-blurred edges
    ...

def synth_shadow_overlay(rgb, shadow_polygons):
    # darken polygon regions by 0.3–0.5 luminance
    ...

def synth_water_expansion(rgb_before, rgb_after, water_polygon):
    # paint water polygon onto rgb_after with realistic blue-green
    ...

# generate combinations; auto-label triage outputs
```

**Per-example structure:**

```json
{
  "image_paths": ["chip_42_before.png", "chip_42_after.png"],
  "system_prompt": "<frozen Role 1 system prompt>",
  "user_text": "Numeric features:\n  area_ha: 1.8\n  ΔNDVI: -0.32\n  ...",
  "expected_output_json": {
    "visible_clouds_over_30_pct": false,
    "visible_cloud_shadows": true,
    "visible_smoke_or_haze": false,
    "geometric_agriculture_pattern": false,
    "open_water_or_flooding": false,
    "recent_bare_ground_visible": true,
    "image_quality": "fair",
    "concurrence": "concur_with_caveat",
    "false_positive_risks": ["shadow"],
    "notes": "Cloud shadow visible in northwest quadrant of the after image."
  },
  "metadata": {
    "source": "simsat_aug",
    "biome": "humid_tropical",
    "augmentation": ["shadow_overlay"],
    "annotator": "auto",
    "annotator_confidence": 1.0
  }
}
```

**Train/val/test split:** 70/15/15, stratified by source × biome.

**Success criteria:** Per-binary-question accuracy ≥ 0.90 on real-Sentinel-2 test split. False-positive risks F1 ≥ 0.75. Schema validity ≥ 0.99.

### 25.3 Role 2 — Classification Confirmation

**Goal:** Multi-class agree/disagree decision.

**Dataset size:** 2,000–5,000 examples.

**Composition:**

| Source | Count | Description |
|---|---|---|
| Confirmed disturbance pairs | 1,500 | Real Sentinel-2 RGB before/after centered on confirmed events from RADD/GLAD-S2 alert layers, type labeled by majority of expert reviewers. Each example presents the rule-based prediction (sometimes correct, sometimes wrong) so the model learns to agree/disagree. |
| Synthetic disagreement examples | 1,000 | Take a confirmed event and pair it with a deliberately-wrong rule-based label; expected output is `disagree_*`. |
| Caveat cases | 500 | Marginal-quality images where `agree_with_caveat` is correct. |

**Per-example structure:**

```json
{
  "image_paths": ["before.png","after.png"],
  "user_text": "Rule-based classification: clear_cut\nClassifier top-1: clear_cut (p=0.78)\nClassifier top-2: agriculture_or_harvest (p=0.15)\nArea: 2.35 ha; ΔNDVI: -0.41",
  "expected_output_json": {
    "rule_based_classification_shown": "clear_cut",
    "visual_assessment": "agree",
    "alternative_type": "clear_cut",
    "confidence_in_visual": 0.84,
    "evidence": [
      "patch boundary is sharp and consistent with rapid clearing",
      "no signs of fire or water"
    ],
    "notes": "Visual matches rule-based classification."
  }
}
```

**Success criteria:** Per-class accuracy ≥ 0.80 on test split. The model must specifically resist promoting selective_logging to clear_cut (this is a measured asymmetric error rate; should be < 5%).

### 25.4 Role 3 — Narrative

**Goal:** Produce evidence-packet narratives in the project's voice.

**Dataset size:** 200–500 examples (smaller; style transfer).

**Composition:**

| Source | Count | Description |
|---|---|---|
| Hand-written narratives | 200 | Domain-expert-written narratives for varied alerts. |
| Style-transferred | 200 | Take real GFW / WRI / RAISG case-summary text; rewrite into our four-section format with our legal disclaimer. |
| Edge cases | 100 | Low-quality alerts, overlay conflicts, unusual disturbance types. |

**Per-example structure:** the four narrative fields plus `evidence_citations` plus the mandatory `legal_disclaimer` constant string. The model must learn never to omit the disclaimer.

**Success criteria:** Schema validity 1.0. Disclaimer present 1.0. Human-rater preference > 0.7 vs. zero-shot baseline.

### 25.5 Role 4 — Map Reading

**Dataset size:** 1,000–2,000 examples.

**Composition:**

- Rendered alert maps (real and synthetic), annotated with QA pairs.
- Question categories: distance-to-nearest-X, what-color-means-Y, where-is-Z-relative-to-W.
- Label generation: programmatic (the rendering pipeline knows the answer).

**Per-example structure:** map image + question + expected answer + confidence + caveats.

**Success criteria:** Per-question accuracy ≥ 0.85.

### 25.6 Role 5 — Reviewer Chat

**Dataset size:** 1,000–3,000 turns.

**Composition:**

- Synthetic dialogues constructed from Alert JSONs + canned reviewer questions ("is this consistent with selective logging?", "summarize the case for enforcement review", "what is the area at risk").
- Outputs cite the relevant Alert JSON fields.
- Hand-curated edge cases: reviewer asks an out-of-scope question; expected output politely refuses with explanation.

**Success criteria:** Faithfulness (every claim is supported by `cited_fields`) ≥ 0.95. Refusal rate on out-of-scope questions ≥ 0.95.

### 25.7 Role 6 — Ground Photo Cross-Check

**Dataset size:** 500–1,500 examples (smaller; data is harder to obtain).

**Composition:**

- Pairs: satellite RGB chip + ground photo at the same location.
- Sources: Mapillary, OpenStreetCam, RAISG community photos, hand-collected.
- Labels: hand-annotated `ground_consistent_with_satellite` boolean and discrepancies list.

**Per-example structure:** two images + expected disturbance type + location + expected output JSON.

**Success criteria:** Accuracy on `consistent_with_satellite` ≥ 0.80 on held-out test set.

### 25.8 Role 7 — Daily Summary

**Dataset size:** 200–500 examples.

**Composition:**

- Aggregated alert tables (synthetic and real) + hand-written reference summaries.
- Reference summaries follow a fixed structure: opening sentence with totals, AOIs of concern, false-positive patterns, recommended next steps.

**Success criteria:** Human-rater preference > 0.7 vs. zero-shot. No hallucinated AOI names (measured by checking that all AOI names in the output appear in the input table).

### 25.9 Multi-Task Combined Adapter

**After roles 1–3 are individually viable**, train a single LoRA on all three. Mix ratio: 50% Role 1 (the most data-heavy and most-called), 30% Role 2, 20% Role 3. Add task tag to the system prompt (`[TASK: TRIAGE]`, `[TASK: CLASSIFY]`, `[TASK: NARRATE]`).

**Success criteria for combined adapter:** No more than 5% per-role regression vs. role-specific adapters; storage savings (one adapter instead of three); inference simplicity.

### 25.10 Active-Learning Loop

After deployment, every reviewer decision becomes a candidate label:

```
review.decision == "confirmed_disturbance"
  → Patch features + RGB chip + final disturbance_type
  → goes into labels/active/positive/

review.decision == "dismissed_false_positive"
  → goes into labels/active/negative/  with reviewer-chosen FP risk
```

Quarterly: retrain LightGBM and (if data volume allows) LoRA adapters on a mixed dataset (synthetic + active labels). Track regression on a fixed test set.

---

## 26. Testing Strategy

### 26.1 Unit Tests

Every public function in `fg-core`, `fg-raster`, `fg-detect`, `fg-legal`, `fg-evidence` has unit tests. Run with `cargo test`.

### 26.2 Property Tests

`proptest`-based:

- `safe_div(a, b)` produces no NaN/Inf for any `(a, b) ∈ f32`.
- `score::clamped(x)` is in `[0,1]` for all `x: f32`.
- `BBox::contains(p) == true` iff geometry contains.
- `extract_patches(empty_mask) == []`.

### 26.3 Golden Tests

For each pipeline stage, frozen inputs in `crates/fg-test-fixtures/data/` and expected outputs. CI fails if outputs change without an explicit "regenerate goldens" step.

### 26.4 Mock Implementations

- `MockSimSatClient`: returns canned BandStacks per (lon, lat, timestamp).
- `MockVlmClient`: returns canned VlmReports per (role, prompt_hash).
- `MockEmbedder`: returns deterministic embeddings.
- `MockClassifier`: returns canned outputs.

These let `fg-pipeline` integration tests run with no external services.

### 26.5 Scenario Tests (Appendix B)

For each named scenario (`A1_clear_cut`, `B1_partial_cloud`, etc.):

```
test scenario:
  given: synthetic SimSat tile + AOI + scenario inputs
  when:  pipeline runs end-to-end
  then:
    - alert level is exactly the expected level
    - disturbance_type is exactly the expected type
    - false_positive_risks is a superset of expected risks
    - evidence packet exists and verifies
    - audit log has expected entries
```

### 26.6 Schema Conformance

`cargo xtask schema-check`:

1. Generate JSON Schemas from Rust types via `schemars`.
2. Diff against canonical schemas in `schemas/`.
3. Fail build on any diff.

### 26.7 End-to-End

`tests/e2e/`:

1. Spin up a local SimSat (or its mock).
2. Spin up llama-server (or `MockVlmClient`).
3. Run the pipeline.
4. Hit dashboard routes.
5. Verify HTML and JSON responses.

Run nightly in CI on the demo profile.

---

## 27. Success Criteria

Success is measurable. Tomorrow's MVP is judged by **routing correctness**, not synthetic precision.

### 27.1 MVP (Day 1)

| Criterion | Target |
|---|---|
| Pipeline runs end-to-end on 3 demo tiles in < 5 min | yes/no |
| All scenario tests A1–A4 pass (clear-cut, agriculture, water, cloud-shadow) | 4/4 |
| Schema conformance check passes | yes/no |
| `fg packet-verify` succeeds on every generated packet | yes/no |
| Dashboard renders alert queue + map + alert detail | yes/no |
| Audit log has every domain event | yes/no |
| No `unwrap()` in production paths | grep check |
| `cargo clippy -- -D warnings` clean | yes/no |
| VLM Roles 1, 2, 3 schema validity ≥ 0.95 on fixtures | ≥ 0.95 |
| No alert ever exceeds level set by deterministic ceiling | invariant test |

### 27.2 Week 2

| Criterion | Target |
|---|---|
| LightGBM trained on synthetic data; integrated; toggleable | yes |
| Role 4 (map reading) integrated | yes |
| All 14 scenarios in Appendix B pass | 14/14 |
| Pipeline parallelizes across tiles | measured speedup |
| Reviewer dashboard fully functional | yes |
| Replay tool reproduces alerts byte-for-byte | yes |

### 27.3 Month 2 (post-real-data port)

| Criterion | Target |
|---|---|
| Real Sentinel-2 STAC ingestion working | yes |
| s2cloudless-equivalent cloud mask integrated | yes |
| Multi-observation confirmation enforced for `enforcement_review` | yes |
| At least one biome stratum (humid tropical) has thresholds calibrated against RADD-confirmed events | yes |
| Role 1 LoRA fine-tuned and deployed | yes |
| Per-binary-question accuracy on real data | ≥ 0.85 |
| End-to-end alert latency on a single tile | < 30 s |

### 27.4 Operational (when in regular use)

| Criterion | Target |
|---|---|
| `enforcement_review` precision against expert review | ≥ 0.85 |
| Reviewer agreement κ on alert disposition | ≥ 0.7 |
| % of alerts with complete provenance | 100% |
| % of evidence packets that verify | 100% |
| Mean time from imagery acquisition to alert | < 1 hr |
| False-positive risk recall on cloud/shadow | ≥ 0.9 |

These targets are aspirational; the MVP does not claim them.

---

## 28. Build Order and Milestones

### Day 1 (single developer, focused)

| Hour | Task |
|---|---|
| 1 | `fg-core`: IDs, geo, bands, errors. |
| 2 | `fg-simsat`: client + types. |
| 3 | `fg-raster`: indices + delta + statistics. |
| 4 | `fg-detect`: masks + connected components + scoring (deterministic only). |
| 5 | `fg-legal`: load real GeoJSON + spatial index + evaluate. |
| 6 | `fg-db`: migrations + repositories (alert + observation + change_event + audit_log). |
| 7 | `fg-vlm`: llama-server client + Role 1 + Role 3 (zero-shot). |
| 8 | `fg-evidence`: build packet + hash + render 8-panel. |
| 9 | `fg-pipeline`: orchestrate one tile. |
| 10 | `fg-server`: minimal axum routes + 2 templates. |
| 11 | `fg-cli`: `run`, `serve`, `migrate`, `packet-verify`. |
| 12 | Scenario tests A1–A4. |
| 13 | Smoke run end-to-end; fix issues. |

### Day 2–4

- `fg-embed` with Clay v1 ONNX export.
- `fg-classify` with synthetic-trained LightGBM.
- VLM Role 2 + asymmetric judgment integration.
- Full dashboard: alert detail page, reviewer actions, htmx forms, SSE.
- Audit log surfaced in UI.

### Week 2

- All 14 scenario tests passing.
- Roles 4, 5, 7.
- Replay tool.
- Synthetic-data LoRA dataset generator.

### Week 3–4

- Real Sentinel-2 STAC port.
- s2cloudless integration.
- Role 6 + ground-photo upload UI.
- Multi-observation confirmation.

### Month 2

- LoRA fine-tuning roles 1, 2, 3.
- Production deployment (Postgres/PostGIS migration documented).

---

## 29. Operational Considerations

### 29.1 Logging

- `tracing` crate.
- JSON output in production, pretty in dev.
- Spans wrap every aggregate-touching operation.
- Every domain event also becomes a structured log entry.

### 29.2 Metrics

- `metrics` crate with the Prometheus exporter.
- Counters: `alerts_raised_total{level}`, `vlm_calls_total{role}`, `vlm_schema_invalid_total{role}`.
- Histograms: `pipeline_run_duration_seconds`, `vlm_latency_seconds{role}`, `simsat_fetch_duration_seconds`.

### 29.3 Resource Limits

- llama-server container memory limit set to model size + 4 GB headroom.
- ONNX Runtime limited to N CPU threads via env.
- Pipeline parallelism configured to `min(num_tiles, num_cpus / 2)`.

### 29.4 Backup

- SQLite file backed up daily (sqlite3 .backup).
- Evidence packets are immutable once sealed; rsync-replicated.

### 29.5 Failure Modes and Responses

| Failure | Response |
|---|---|
| SimSat unreachable | Pipeline fails fast; run marked failed; alert ops. |
| llama-server unreachable | Pipeline continues with deterministic-only alerts; VlmReports recorded with `model.kind = Unavailable`. |
| ONNX inference fails | Embedding score = `None`; alert proceeds without it. |
| LightGBM model missing | Fall back to rule-based disturbance type; log warning. |
| DB write fails | Pipeline aborts; transaction rollback; ops alerted. |
| Schema-invalid VLM output | One retry; on second failure, persist with `schema_valid=false`; alert defaults to deterministic. |
| Disk full (evidence) | Pipeline pauses new alerts; ops alerted. |

---

## 30. Real-Data Port Plan

When porting from SimSat to real Sentinel-2 (post-MVP):

### 30.1 Replace `fg-simsat` with `fg-stac`

- Element 84 Earth Search or Microsoft Planetary Computer STAC.
- Fetch L2A items by bbox + datetime range.
- Apply scaling per L2A spec (BOA reflectance × 10000 → divide by 10000).

### 30.2 Add `fg-cloud-mask`

- Per-pixel cloud probability from `s2cloudless` (export to ONNX).
- Apply Sen2Cor SCL band logic if SCL is available.

### 30.3 Recalibrate Thresholds

Against RADD-confirmed Congo Basin events for humid tropical. Against MapBiomas Alerta for Cerrado/dry. Document each calibration run.

### 30.4 Enforce Multi-Observation Confirmation

Require `detection_count ≥ 2` for `enforcement_review` (trivial schema change; logic already in place).

### 30.5 Add Sentinel-1 SAR Confounder Check (optional)

- Fetch S1 GRD VH at the same location.
- Compute backscatter delta.
- If optical disturbance lacks SAR corroboration, add `cloud` or `temporal_mismatch` to false-positive risks.

### 30.6 Acceptance

The system is "ported" when:

- All scenario tests pass on real data (with adapted thresholds).
- 50 hand-labeled alerts achieve ≥ 0.80 expert agreement.
- Latency budget holds.
- All architectural invariants from this document still hold.

---

## Appendix A — Reference Index Formulas

| Index | Formula | Useful range | Notes |
|---|---|---|---|
| NDVI | (NIR − Red) / (NIR + Red) | [-1, 1] | Vegetation greenness |
| NDMI | (NIR − SWIR1) / (NIR + SWIR1) | [-1, 1] | Canopy moisture |
| NBR  | (NIR − SWIR2) / (NIR + SWIR2) | [-1, 1] | Burn / disturbance |
| BSI  | ((SWIR1 + Red) − (NIR + Blue)) / ((SWIR1 + Red) + (NIR + Blue)) | [-1, 1] | Bare soil |
| NDWI | (Green − NIR) / (Green + NIR) | [-1, 1] | Water (McFeeters) |
| NDRE | (NIR − RedEdge2) / (NIR + RedEdge2) | [-1, 1] | Red-edge stress |
| EVI  | 2.5 · (NIR − Red) / (NIR + 6·Red − 7.5·Blue + 1) | [-1, 1] | Optional; reduces atmospheric saturation |
| MSAVI | (2·NIR + 1 − sqrt((2·NIR + 1)² − 8·(NIR − Red))) / 2 | [-1, 1] | Optional; soil-adjusted |

All formulas use `safe_div` with `eps=1e-6`. Inputs are reflectance in `[0,1]`.

---

## Appendix B — Synthetic Test Scenarios

Fourteen named scenarios. Each is a fixture in `crates/fg-test-fixtures/data/scenarios/<name>/`.

| ID | Name | Expected level | Expected type | Expected FP risks |
|---|---|---|---|---|
| A1 | Clear-cut in protected area, clear sky | `enforcement_review` | `clear_cut` | none |
| A2 | Clear-cut with valid permit overlay | `investigate` | `clear_cut` | none |
| A3 | Clear-cut on cropland (no protected) | `watch` | `agriculture_or_harvest` | `seasonal_crop_harvest` |
| A4 | Stable forest, no change | `none` | `none` | none |
| B1 | Disturbance + 25% cloud cover in current | `investigate` (downgraded) | `clear_cut` | `cloud` |
| B2 | Disturbance with 15% cloud shadow | `investigate` (downgraded) | `clear_cut` | `shadow` |
| B3 | Geometric harvest patch + protected area | `watch` (downgraded by Role 1) | `agriculture_or_harvest` | `seasonal_crop_harvest` |
| B4 | Flooding inside forest | `watch` | `flood_or_water_change` | `water` |
| C1 | Selective-logging gaps (multi small patches) | `watch` | `selective_logging` | none |
| C2 | Logging-road extension (elongated patch) | `watch` | `logging_road` | none |
| C3 | Burn scar (low NBR, irregular boundary) | `investigate` | `burn_scar` | `fire` |
| C4 | Mining-style clearing (sediment + ponds) | `enforcement_review` | `mining` | none |
| D1 | Tile-edge no-data artifact | `none` | `none` | `sensor_or_tile_artifact` |
| D2 | Co-registration drift mimicking road | `watch` | `unknown_disturbance` | `georegistration_mismatch` |

For each scenario, the fixture provides:
- Two synthetic BandStacks (current, baseline).
- AOI overlap (which legal layers).
- Expected outputs for: candidate mask, patch list, alert level, type, FP risks, VLM Role 1 outputs, evidence packet manifest.

---

## Appendix C — Crate Dependency Matrix

| Crate | reqwest | tokio | sqlx | gdal | ndarray | image | geo | ort | lightgbm3 | axum | askama | serde | thiserror |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| fg-core           |   |   |   |   |   |   | ✓ |   |   |   |   | ✓ | ✓ |
| fg-simsat         | ✓ | ✓ |   |   | ✓ | ✓ |   |   |   |   |   | ✓ | ✓ |
| fg-raster         |   |   |   | ✓ | ✓ | ✓ | ✓ |   |   |   |   | ✓ | ✓ |
| fg-detect         |   |   |   |   | ✓ | ✓ | ✓ |   |   |   |   | ✓ | ✓ |
| fg-embed          |   | ✓ |   |   | ✓ |   |   | ✓ |   |   |   | ✓ | ✓ |
| fg-classify       |   |   |   |   | ✓ |   |   |   | ✓ |   |   | ✓ | ✓ |
| fg-legal          |   |   |   |   |   |   | ✓ |   |   |   |   | ✓ | ✓ |
| fg-vlm            | ✓ | ✓ |   |   |   | ✓ |   |   |   |   |   | ✓ | ✓ |
| fg-evidence       |   |   |   |   |   | ✓ | ✓ |   |   |   |   | ✓ | ✓ |
| fg-db             |   | ✓ | ✓ |   |   |   |   |   |   |   |   | ✓ | ✓ |
| fg-pipeline       |   | ✓ |   |   |   |   |   |   |   |   |   | ✓ | ✓ |
| fg-server         |   | ✓ |   |   |   |   |   |   |   | ✓ | ✓ | ✓ | ✓ |
| fg-cli            |   | ✓ |   |   |   |   |   |   |   |   |   | ✓ | ✓ |

Pinned versions live in the workspace `Cargo.toml`. Update via `cargo update -p <crate>` and re-run the full test suite.

---

**End of BUILD.md.**

Anyone holding this document and the SimSat repo can implement the entire system without consulting other sources. If anything in this document is ambiguous or unimplementable, file an ADR amendment before writing code.
