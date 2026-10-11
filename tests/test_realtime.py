import threading
import unittest
from dataclasses import replace
from types import SimpleNamespace
from unittest.mock import patch

from diablo2.common.config import CaptureConfig
from diablo2.common.realtime import DecisionSnapshot, RealtimeRuntimeState, RealtimeVisionRuntime, RuntimeFrame, VisionSnapshot


class RealtimeTests(unittest.TestCase):
    def setUp(self):
        self.now = [10.0]
        self.state = RealtimeRuntimeState(clock=lambda: self.now[0])
        self.target = {"left": 100, "top": 50, "width": 1922, "height": 1140}

    def frame(self, sequence=0, **changes):
        return replace(RuntimeFrame(sequence, object(), self.now[0], dict(self.target), self.state.generation), **changes)

    def vision(self, frame, *, channel="fast", payload="visible", **changes):
        return replace(VisionSnapshot(channel, payload, frame.sequence_id, frame.captured_at, self.now[0], frame.generation), **changes)

    def decision(self, frame, **changes):
        return replace(DecisionSnapshot("move", self.now[0], frame.sequence_id, frame.captured_at, frame.generation), **changes)

    def runtime(self, **kwargs):
        return RealtimeVisionRuntime(
            CaptureConfig(),
            threading.Event(),
            lambda frame: None,
            lambda frame: None,
            lambda snapshot: None,
            clock=lambda: self.now[0],
            **kwargs,
        )

    def test_wall_clock_changes_do_not_change_freshness(self):
        frame = self.frame(wall_captured_at=1_800_000_000.0)
        self.assertTrue(self.state.publish_frame(frame))
        self.now[0] = 10.2
        with patch("diablo2.common.realtime.time.time", return_value=-10_000):
            snapshot = self.state.snapshot()
        self.assertAlmostEqual(snapshot.sampled_at - snapshot.latest_frame.captured_at, 0.2)
        self.assertEqual(snapshot.latest_frame.wall_captured_at, 1_800_000_000.0)

    def test_invalidate_discards_old_capture_vision_and_decision(self):
        frame = self.frame()
        self.state.publish_frame(frame)
        old_snapshot = self.state.snapshot()
        self.state.publish_vision("fast", self.vision(frame))
        self.state.publish_decision(self.decision(frame))
        self.assertEqual(self.state.invalidate(), 1)
        self.assertFalse(self.state.is_current_generation(old_snapshot))
        self.assertFalse(self.state.publish_frame(replace(frame, sequence_id=1)))
        self.now[0] = 11
        self.assertTrue(self.state.publish_frame(self.frame(2)))
        self.assertFalse(self.state.publish_vision("fast", self.vision(frame)))
        self.assertFalse(self.state.publish_decision(self.decision(frame)))
        snapshot = self.state.snapshot()
        self.assertIsNone(snapshot.fast_vision)
        self.assertIsNone(snapshot.latest_decision)
        self.assertEqual([item.sequence_id for item in snapshot.recent_frames], [2])

    def test_window_rectangle_change_starts_a_new_generation(self):
        before = self.frame()
        self.state.publish_frame(before)
        self.state.publish_vision("slow", self.vision(before, channel="slow"))
        self.now[0] = 11
        moved = self.frame(1, target={**self.target, "left": 200})
        self.assertTrue(self.state.publish_frame(moved))
        snapshot = self.state.snapshot()
        self.assertEqual(snapshot.generation, 1)
        self.assertEqual(snapshot.latest_frame.generation, 1)
        self.assertIsNone(snapshot.slow_vision)
        self.assertFalse(self.state.publish_vision("slow", self.vision(before, channel="slow")))

    def test_reversed_future_and_duplicate_results_cannot_replace_newer_evidence(self):
        first = self.frame()
        self.state.publish_frame(first)
        self.now[0] = 11
        second = self.frame(1)
        self.state.publish_frame(second)
        current = self.vision(second)
        self.assertTrue(self.state.publish_vision("fast", current))
        for rejected in (
            self.vision(first),
            current,
            replace(current, source_sequence_id=2),
            replace(current, source_captured_at=12),
            replace(current, produced_at=12),
        ):
            self.assertFalse(self.state.publish_vision("fast", rejected))
        self.assertIs(self.state.snapshot().fast_vision, current)
        self.assertFalse(self.state.publish_frame(self.frame(2, captured_at=12)))
        self.assertFalse(self.state.publish_frame(self.frame(1)))

    def test_missing_observation_clears_channel_and_cannot_resurrect_old_result(self):
        first = self.frame()
        self.state.publish_frame(first)
        old = self.vision(first)
        self.state.publish_vision("fast", old)
        self.now[0] = 11
        second = self.frame(1)
        self.state.publish_frame(second)
        self.assertTrue(self.state.publish_vision("fast", self.vision(second, payload=None)))
        self.assertIsNone(self.state.snapshot().fast_vision)
        self.assertFalse(self.state.publish_vision("fast", old))
        with self.assertRaises(ValueError):
            self.state.publish_vision("slow", self.vision(second))

    def test_latest_frame_and_buffer_stay_bounded_when_consumer_is_delayed(self):
        for index in range(20):
            self.now[0] += 0.1
            self.state.publish_frame(self.frame(index))
        snapshot = self.state.snapshot()
        self.assertEqual(snapshot.latest_frame.sequence_id, 19)
        self.assertEqual([frame.sequence_id for frame in snapshot.recent_frames], [15, 16, 17, 18, 19])

    def test_clock_regression_is_an_error_instead_of_negative_age(self):
        self.state.snapshot()
        self.now[0] = 9
        with self.assertRaisesRegex(RuntimeError, "backwards"):
            self.state.snapshot()

    def test_capture_uses_start_time_closes_and_discards_capture_invalidated_in_flight(self):
        runtime = self.runtime()
        calls, closed = [], []

        class FakeCapture:
            target = self.target

            def grab(capture):
                calls.append(len(calls))
                self.now[0] += 1
                if len(calls) == 1:
                    runtime.invalidate_observations()
                else:
                    runtime.stop_event.set()
                return SimpleNamespace(frame=object(), timestamp=1_800_000_000.0)

            def close(capture):
                closed.append(True)

        runtime.capture_factory = lambda config: FakeCapture()
        runtime._capture_loop()
        latest = runtime.state.latest_frame()
        self.assertEqual(latest.sequence_id, 1)
        self.assertEqual(latest.generation, 1)
        self.assertEqual(latest.captured_at, 11)
        self.assertEqual(latest.wall_captured_at, 1_800_000_000.0)
        self.assertEqual(closed, [True])

    def test_capture_failure_closes_source_and_signals_error(self):
        errors, closed = [], []
        runtime = self.runtime(error_handler=lambda stage, error: errors.append((stage, str(error))))

        class FakeCapture:
            def grab(capture):
                raise RuntimeError("capture failed")

            def close(capture):
                closed.append(True)

        runtime.capture_factory = lambda config: FakeCapture()
        runtime._capture_loop()
        self.assertTrue(runtime.stop_event.is_set())
        self.assertEqual(errors, [("capture", "capture failed")])
        self.assertEqual(closed, [True])

    def test_slow_worker_cannot_publish_previous_generation_after_new_capture(self):
        runtime = self.runtime()
        runtime.state.publish_frame(self.frame())
        entered, resume = threading.Event(), threading.Event()

        def processor(frame):
            entered.set()
            if not resume.wait(2):
                raise RuntimeError("test worker timeout")
            runtime.stop_event.set()
            return "old enemy position"

        worker = threading.Thread(target=runtime._vision_loop, args=("slow", processor, 0))
        worker.start()
        try:
            self.assertTrue(entered.wait(2))
            runtime.invalidate_observations()
            self.now[0] = 11
            runtime.state.publish_frame(replace(self.frame(1), generation=runtime.state.generation))
        finally:
            resume.set()
            worker.join(2)
            runtime.stop_event.set()
        self.assertFalse(worker.is_alive())
        self.assertIsNone(runtime.state.snapshot().slow_vision)

    def test_decision_finishing_after_invalidation_is_not_accepted(self):
        runtime = self.runtime()
        runtime.state.publish_frame(self.frame())

        def decide(snapshot):
            runtime.invalidate_observations()
            runtime.stop_event.set()
            return "old movement result"

        runtime.decision_fn = decide
        runtime._decision_loop()
        self.assertIsNone(runtime.state.snapshot().latest_decision)

    def test_vision_worker_skips_backlog_and_clears_none_result(self):
        runtime = self.runtime()
        for index in range(10):
            self.now[0] += 0.1
            runtime.state.publish_frame(self.frame(index))
        latest = runtime.state.latest_frame()
        runtime.state.publish_vision("fast", self.vision(latest))
        self.now[0] += 0.1
        runtime.state.publish_frame(self.frame(10))
        seen = []

        def process(frame):
            seen.append(frame.sequence_id)
            runtime.stop_event.set()
            return None

        runtime._vision_loop("fast", process, 0)
        self.assertEqual(seen, [10])
        self.assertIsNone(runtime.state.snapshot().fast_vision)


if __name__ == "__main__":
    unittest.main()
