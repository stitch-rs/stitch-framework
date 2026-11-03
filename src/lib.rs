//! # Stitch.rs (`stitch-rs`)
//!
//! Zero-Cost, Monomorphic U-Cycle Middleware Pipeline Framework implementing
//! **The Sewing Machine Architecture (SMA)**.
//!
//! > **Result = Material + Intent + Work**
//!
//! - **Material**: The passive domain context/state ([`Blackboard`]).
//! - **Intent**: The desire entering the pipeline ([`FlowControl`]).
//! - **Work**: The compile-time monomorphic pipeline ([`Pipeline`]) driving the U-cycle:
//!   - Phase 1: Descent (`on_enter`: Proceed, ShortCircuit, or Halt)
//!   - Point of Puncture: Execution at [`Terminal`]
//!   - Phase 2: Ascent (`on_exit`: Reactions, Diffs, Telemetry)

#![cfg_attr(not(feature = "std"), no_std)]

pub use stitch_core::*;

#[cfg(feature = "macros")]
pub use stitch_macros::*;

#[cfg(feature = "macros")]
pub use stitch_macros as stitch;

/// Unified prelude re-exporting all core traits, tokens, and macros.
pub mod prelude {
    pub use stitch_core::prelude::*;

    #[cfg(feature = "macros")]
    pub use stitch_macros::{
        adapter, blackboard, data, define_pipeline, entity, event, id, layer, pipeline, port,
        provider, terminal, token, value_object, wire_pipeline,
    };
}

#[cfg(all(test, feature = "macros"))]
mod tests {
    use super::prelude::*;

    #[blackboard]
    #[repr(C, align(64))]
    struct MockContext {
        val: u64,
    }
    impl Blackboard for MockContext {}

    #[token]
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    struct MockToken(u64);
    impl StitchToken for MockToken {
        fn raw_u64(&self) -> u64 {
            self.0
        }
    }

    #[id]
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    struct MockId(MockToken);
    impl StitchId for MockId {
        type Token = MockToken;
        fn token(&self) -> Self::Token {
            self.0
        }
    }

    #[test]
    fn test_facade_and_macros_integration() {
        let token = MockToken(42);
        let id = MockId(token);
        assert_eq!(id.raw_u64(), 42);
        assert!(core::mem::align_of::<MockContext>() >= 64);
    }

    define_pipeline! {
        crate: crate,
        name: Sample,
        context: MockContext,
        intent: u32,
        outcome: (),
        error: (),
        vis: pub,
    }

    struct MockSampleLayer;
    impl SampleLayer for MockSampleLayer {
        fn on_enter(&self, ctx: &mut MockContext, intent: &u32) -> FlowControl<(), (), ()> {
            ctx.val += *intent as u64;
            FlowControl::Proceed(())
        }
        fn on_exit(&self, _ctx: &mut MockContext, _intent: &u32, outcome: &mut Result<(), ()>) {
            assert!(outcome.is_ok());
        }
    }

    #[test]
    fn test_define_pipeline_macro_generated_types() {
        let mut ctx = MockContext { val: 10 };
        let pipeline = SamplePipeline::new().with_layer(std::sync::Arc::new(MockSampleLayer));
        let res = pipeline.dispatch(&mut ctx, &5);
        assert!(res.is_ok());
        assert_eq!(ctx.val, 15);
    }

    struct LayerOne;
    impl Layer<MockContext, u32, u32, ()> for LayerOne {
        fn on_enter(&self, ctx: &mut MockContext, intent: u32) -> FlowControl<u32, u32, ()> {
            ctx.val += 100;
            FlowControl::Proceed(intent + 1)
        }
        fn on_exit(&self, ctx: &mut MockContext, outcome: &mut Result<u32, ()>) {
            ctx.val += 1;
            if let Ok(v) = outcome {
                *v += 10;
            }
        }
    }

    struct LayerTwo;
    impl Layer<MockContext, u32, u32, ()> for LayerTwo {
        fn on_enter(&self, ctx: &mut MockContext, intent: u32) -> FlowControl<u32, u32, ()> {
            ctx.val += 200;
            FlowControl::Proceed(intent * 2)
        }
        fn on_exit(&self, ctx: &mut MockContext, _outcome: &mut Result<u32, ()>) {
            ctx.val += 2;
        }
    }

    struct SimpleTerminal;
    impl Terminal<MockContext, u32, u32, ()> for SimpleTerminal {
        fn execute(&mut self, ctx: &mut MockContext, intent: u32) -> Result<u32, ()> {
            ctx.val += 1000;
            Ok(intent + 5)
        }
    }

    #[pipeline(
        context = MockContext,
        intent = u32,
        outcome = u32,
        error = (),
        layers = [LayerOne, LayerTwo],
        terminal = SimpleTerminal,
        crate = crate,
    )]
    struct TestNewtypePipeline;

    #[test]
    fn test_attribute_newtype_pipeline_execution() {
        let mut ctx = MockContext { val: 0 };
        let mut pipe = TestNewtypePipeline::new(LayerOne, LayerTwo, SimpleTerminal);

        let res = pipe.run_cycle(&mut ctx, 10);
        assert_eq!(res, Ok(37));
        assert_eq!(ctx.val, 1303);
    }
}
