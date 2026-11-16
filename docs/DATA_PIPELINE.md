# Stitch Ecosystem Data Pipeline & Responsibility Matrix

This specification defines the strict separation of concerns, terminology, and end-to-end data lifecycle across the **stitch-rs** ecosystem.

---

## 1. Conceptual Taxonomy (Glossary of Primitives)

To prevent cognitive friction and eliminate over-engineered "Swiss Army knife" monoliths, each layer of the ecosystem maintains an exclusive normative responsibility:

| Term | Architectural Role | Responsibility | Strict Anti-Goals | Key Crate |
| :--- | :--- | :--- | :--- | :--- |
| **Monomorphic Pipeline** | Orchestration & Control Flow (L7) | Static compile-time U-Cycle composition (`on_enter` $\to$ `execute` $\to$ `on_exit`). Flattens into monomorphic inlined assembly (~1.30 ns). | Zero knowledge of network bytes, sockets, serialization schemas, or ring buffers. Operates strictly on typed `Blackboard` and `Intent`. | `stitch-framework` (`stitch-core`) |
| **Data Layout & Codec** | Presentation & Memory Layout (L6) | In-memory structural geometry (Columnar SoA, bit-packing, branchless PrefixVarint, ZigZag, Delta, RLE, IEEE-754 Snorm/F16). Maximizes entropy density. | Zero knowledge of network headers, UDP packets, framing envelopes, socket IO, or inter-thread queues. | `stitch-data` |
| **Wire Framing & Transport Bus** | Session & Physical Framing (L4–L5) | Physical byte stream envelope: 2-byte prefix (`0xCB01`), AVX2 SIMD stream recovery (35 GiB/s), hardware Castagnoli CRC32, chunking, vectored IO (`writev`). | Zero parsing of business payload fields. Zero inter-thread core routing. | `stitch-wire` |
| **Channel Interconnect** | Intra-Process Delivery (L3) | Lock-free SPSC ring buffers (64-byte L1D cache-padded cursors, shadow caching), compile-time unrolled static multicast Fan-Out, and port adapters (Tokio / NATS). | Zero data compression, bit manipulation, or frame checksum calculations. | `stitch-channel` |
| **External Transport** | OS / Network Transport (L1–L2) | Physical I/O medium and message brokers: UDP sockets, Shared Memory (`shm`), IPC pipes, Unix domain sockets, NATS, Kafka. | Provided by OS kernels or external runtime drivers; Stitch components interface via pluggable SPI port adapters. | `std::net`, `tokio`, `nats.rs` |

---

## 2. End-to-End Data Pipeline Architecture

Under the **Sewing Machine Architecture (SMA)**, data flows through specialized, strictly bounded phases:

```text
+-------------------------------------------------------------------------------+
| Stage 1: Domain Logic & Orchestration (L7 Control Flow)                       |
|   Domain Blackboard & Intent ---> stitch-core U-Cycle (on_enter / on_exit)   |
+-------------------------------------------------------------------------------+
                                      |
                                      v
+-------------------------------------------------------------------------------+
| Stage 2: In-Memory Layout & Compression (L6 Codec)                            |
|   stitch-data (#[derive(Record)], PrefixVarint, Delta, RLE, BitPack)          |
|   Result: Dense zero-allocation payload bytes (e.g. 27B vs 128B raw)          |
+-------------------------------------------------------------------------------+
                                      |
                                      v
+-------------------------------------------------------------------------------+
| Stage 3: Wire Framing & Integrity (L4-L5 Session/Transport Framing)           |
|   stitch-wire (Magic 0xCB01 + Flags + Hardware Castagnoli CRC32)             |
|   Result: Corruption-resilient wire envelope with 35.4 GiB/s SIMD resync      |
+-------------------------------------------------------------------------------+
                                      |
                                      v
+-------------------------------------------------------------------------------+
| Stage 4: Inter-Thread Interconnect & Fan-Out (L3 Interconnect)                |
|   stitch-channel (Zero-copy SPSC ring buffers, 64B cache line isolated)       |
|   Static Monomorphic Fan-Out ---> Replay Disk / Network Worker / Tokio Bridge  |
+-------------------------------------------------------------------------------+
                                      |
                                      v
+-------------------------------------------------------------------------------+
| Stage 5: Physical Transport (L1-L2 Physical IO)                               |
|   OS Sockets (UDP writev) / Shared Memory / Remote Brokers                    |
+-------------------------------------------------------------------------------+
```

---

## 3. Responsibility Matrix

| Feature / Architectural Responsibility | `stitch-framework` | `stitch-data` | `stitch-wire` | `stitch-channel` |
| :--- | :---: | :---: | :---: | :---: |
| **Monomorphic U-Cycle Flow** (`on_enter` / `on_exit`) | **Owner (100%)** | Consumer (via layers) | Consumer (via layers) | Consumer (`ChannelLayer`) |
| **64-byte L1D Cache Line Alignment** | `Blackboard` contexts | Columnar SoA blocks | Vectored scratch buffers | Atomic Cursors & Rings |
| **Domain Record Serialization & Codecs** | No | **Owner (100%)** | No | No |
| **Entropy Compression (Delta, RLE, BitPack)** | No | **Owner (100%)** | No | No |
| **Hardware Castagnoli CRC32 Checksum** | No | Schema hash only | **Owner (100%)** (Frame integrity) | No |
| **SIMD Resync Stream Recovery** | No | No | **Owner (100%)** (35.4 GiB/s) | No |
| **Framing Headers (`Magic 0xCB01`, Flags)** | No | No | **Owner (100%)** | No |
| **Lock-free SPSC Inter-Thread Rings** | No | No | No | **Owner (100%)** |
| **Static Monomorphic Multicast Fan-Out** | No | No | No | **Owner (100%)** |
| **External Broker Adapters (Tokio, NATS)** | No | No | No | **Owner (100%)** (`adapters/*`) |
| **Strict `#![no_std]` Native Core** | Yes (`stitch-core`) | Yes (`stitch-data-core`) | Yes (`stitch-wire-core`) | Yes (`stitch-channel-core`) |
| **Dynamic Allocations on Hot Path** | **0 bytes** | **0 bytes** | **0 bytes** | **0 bytes** |

---

## 4. End-to-End Concrete Example: High-Tick Telemetry

Consider a 128 Hz simulation tick or high-frequency order book update:

1. **`stitch-framework`**:
   The engine tick loop invokes the pipeline. An authentication/validation layer inspects the `Blackboard` on descent (`on_enter`). If valid, execution reaches the terminal.
2. **`stitch-data`**:
   The terminal passes domain records to `stitch-data`. `#[derive(Record)]` compacts coordinates via ZigZag Varint and bit-packs status flags into 5 bits. Payload size drops from 128 bytes to 27 bytes without allocating heap memory.
3. **`stitch-wire`**:
   The 27-byte payload is framed via `stitch-wire`. A 2-byte prefix (`0xCB01`) and a 4-byte hardware CRC32 are prepended/appended via scatter-gather vectored slices (`writev`), ensuring line corruption detection.
4. **`stitch-channel`**:
   The framed message enters an SPSC ring buffer in `stitch-channel`. A compile-time unrolled `StaticMulticast` distributes the frame across dedicated worker threads (e.g. disk replay logger, UDP network broadcast, and an asynchronous Tokio telemetry bridge) with zero-copy leases.

---

## 5. Developer & Machine Experience (DX / MX)

- **Predictable Diagnostics**: Clear failure domains — bit corruption in `stitch-data`, wire transport corruption in `stitch-wire`, queue backpressure in `stitch-channel`.
- **Mechanical Sympathy (MX)**: 64-byte L1D cache-line isolation prevents false sharing across cores.
- **Composable Decoupling**: Each crate can be adopted independently without dragging transitive dependencies.
