import json
import subprocess
import sys
import unittest

from lind_demo.model import DemoError, OPERATIONS, World
from lind_demo.runner import Runner
from lind_demo.scenarios import SCENARIOS


class SequenceTests(unittest.TestCase):
    def test_all_reference_sequences_and_operation_coverage(self):
        operations = set()
        for name, spec in SCENARIOS.items():
            with self.subTest(sequence=name):
                world = Runner().run(spec["steps"])
                operations.update(event["op"] for event in world.events)
        self.assertEqual(OPERATIONS - operations, set())

    def test_custom_values_and_expected_errors(self):
        runner = Runner()
        runner.run([
            dict(op="launch", cage=1, image="parent.wasm"),
            dict(op="fork", source=1, child=2, save="barrier"),
            dict(op="tick"),
            dict(op="check", path="last", equals=[]),
            dict(op="unwind", barrier="$barrier"),
            dict(op="signal", cage=1, signal="STOP", tid=1, expect_error="ENOTSUP"),
        ])
        self.assertTrue(runner.world.state()["cages"]["2"]["threads"]["1"]["runnable"])
        with self.assertRaisesRegex(DemoError, "expected ESRCH"):
            runner.execute(dict(op="tick", expect_error="ESRCH"))

    def test_invalid_steps_are_explicit_errors(self):
        runner = Runner()
        invalid = [
            [], {"op": "__class__"}, {"op": []}, {"op": "launch"},
            {"op": "launch", "cage": True, "image": "app.wasm"},
            {"op": "launch", "cage": 1, "image": "app.wasm", "surprise": 2},
            {"op": "unwind", "barrier": "$missing"},
            {"op": "check", "path": "state.missing", "equals": 1},
        ]
        for step in invalid:
            with self.subTest(step=step), self.assertRaises(DemoError) as error:
                runner.execute(step)
            self.assertEqual(error.exception.code, "INPUT")


class ContractTests(unittest.TestCase):
    def test_clone_order_and_private_shared_memory(self):
        world = World()
        world.launch(1, "app.wasm")
        world.map(1, 0, 4)
        world.map(1, 16, 4, shared=True)
        world.write(1, 0, "abcd0000")
        world.protect(1, 0, 4, "")
        barrier = world.fork(1, 2)
        self.assertEqual(world.tick(), [])
        world.unwind(barrier)
        world.protect(2, 0, 4, "rw")
        self.assertEqual(world.read(2, 0, 4)["data_hex"], "abcd0000")
        world.write(2, 16, "01020304")
        self.assertEqual(world.read(1, 16, 4)["data_hex"], "01020304")
        ops = [e["op"] for e in world.events]
        self.assertLess(ops.index("DuplicateThread"), ops.index("TID.writes"))
        self.assertLess(ops.index("TID.writes"), ops.index("3i.unwind"))
        self.assertLess(ops.index("3i.unwind"), ops.index("scheduling.point"))

    def test_staging_cross_runtime_exec_and_sibling_destruction(self):
        world = World()
        world.launch(1, "app.wasm")
        barrier = world.clone(1, 1, 2)
        world.unwind(barrier)
        world.init(10, "new.elf", staging=True)
        world.create(10, run_before_return=True)
        self.assertEqual(world.tick(), [[1, 1], [1, 2]])
        world.signal(1, "STOP")
        world.apply(10, 1)
        self.assertEqual(world.tick(), [])
        self.assertEqual(world.state()["live_threads"], 1)
        world.signal(1, "CONT")
        world.tick()
        steps = [e for e in world.events if e["op"] == "guest.step"]
        self.assertEqual(steps[-1]["worker"], "mpk-worker")
        self.assertEqual(steps[-1]["cage"], 1)
        self.assertEqual(world.cage(1).generation, 2)
        self.assertEqual(set(world.cage(1).threads), {1})
        world.invariant()

    def test_initial_creation_can_run_before_return_but_not_before_accounting(self):
        world = World()
        world.init(1, "app.wasm")
        world.create(1, run_before_return=True)
        ops = [e["op"] for e in world.events]
        self.assertLess(ops.index("lifetime.accounted"), ops.index("CreateThread"))
        self.assertLess(ops.index("CreateThread"), ops.index("guest.step"))
        self.assertLess(ops.index("guest.step"), ops.index("CreateThread.return"))
        world.init(2, "instant.elf", root=True)
        world.create(2, exit_before_return=23)
        self.assertNotIn(2, world.cages)
        self.assertEqual(world.root_results[2], 23)

    def test_source_partial_copy_and_overlap_snapshot(self):
        world = World()
        world.launch(1, "app.wasm")
        world.launch(2, "grate.elf")
        world.map(1, 0, 4)
        world.map(2, 0, 8)
        world.write(1, 0, "01020304")
        result = world.copy(1, 2, 0, 0, 8)
        self.assertEqual(result["copied"], 4)
        self.assertEqual(result["fault"]["owner"], 1)
        result = world.copy(2, 2, 0, 1, 4)
        self.assertIsNone(result["fault"])
        self.assertEqual(world.read(2, 0, 5)["data_hex"], "0101020304")

    def test_self_registration_and_exec_dependency(self):
        world = World()
        world.launch(1, "app.wasm")
        world.register(1, "self", 1)
        invocation = world.call(1, "self")
        self.assertEqual(world.return_handler(invocation, 7), 7)
        with self.assertRaises(DemoError) as error:
            world.exec(1, "new.wasm")
        self.assertEqual(error.exception.code, "EBUSY")
        world.unregister(1, "self")
        world.exec(1, "new.wasm")
        world.invariant()

    def test_target_thread_and_argument_owner_survive_forwarding(self):
        world = World()
        world.launch(1, "app.wasm")
        world.launch(2, "grate.elf")
        barrier = world.clone(1, 1, 7)
        world.unwind(barrier)
        world.register(1, "read", 2)
        arguments = [dict(value=64, owner=1, kind="pointer")]
        invocation = world.call(2, "read", target=1, tid=7, arguments=arguments)
        event = [e for e in world.events if e["op"] == "RunHandler"][-1]
        self.assertEqual((event["caller"], event["target"], event["target_tid"]), (2, 1, 7))
        self.assertEqual(event["arguments"], arguments)
        world.return_handler(invocation)

    def test_forwarded_fork_does_not_duplicate_grate_or_escape_unwind(self):
        world = World()
        world.launch(1, "app.wasm")
        world.launch(2, "grate.elf")
        clone = world.clone(1, 1, 7)
        world.unwind(clone)
        world.register(1, "fork", 2)
        invocation = world.call(1, "fork", tid=7, caller_tid=7)
        barrier = world.fork(1, 3, tid=7, caller=2)
        self.assertEqual(world.cage(3).runtime, "wasm")
        self.assertEqual(world.thread(3, 1).tls, "fresh:7")
        with self.assertRaises(DemoError):
            world.unwind(barrier)
        world.return_handler(invocation)
        world.unwind(barrier)
        self.assertEqual(world.tick(), [[1, 1], [1, 7], [3, 1], [2, 1]])

    def test_thread_directed_signal_is_delivered_to_selected_thread_only(self):
        world = World()
        world.launch(1, "app.wasm")
        barrier = world.clone(1, 1, 7)
        world.unwind(barrier)
        world.signal(1, "USR1", tid=7)
        world.tick()
        delivered = [event for event in world.events if event["op"] == "signal.deliver"]
        self.assertEqual([event["tid"] for event in delivered], [7])

    def test_fork_failure_does_not_change_parent_or_publish_zombie(self):
        for failure, code in [("memory", "ENOMEM"), ("thread", "EAGAIN")]:
            with self.subTest(failure=failure):
                world = World()
                world.launch(1, "app.wasm")
                world.signal(1, "STOP")
                before = world.state()
                with self.assertRaises(DemoError) as error:
                    world.fork(1, 2, fail=failure)
                self.assertEqual(error.exception.code, code)
                self.assertEqual(world.state(), before)
                self.assertEqual(world.zombies, {})

    def test_new_stop_survives_scheduling_barrier(self):
        world = World()
        world.launch(1, "app.wasm")
        barrier = world.fork(1, 2)
        world.signal(1, "STOP")
        world.unwind(barrier)
        self.assertEqual(world.tick(), [[2, 1]])
        world.signal(1, "STOP")
        world.signal(1, "CONT")
        self.assertEqual(world.tick(), [[1, 1], [2, 1]])

    def test_root_exit_does_not_complete_live_descendant_execution(self):
        world = World()
        world.launch(1, "app.wasm")
        barrier = world.fork(1, 2)
        world.unwind(barrier)
        world.exit_thread(1, 1, status=3)
        self.assertFalse(world.state()["complete"])
        self.assertEqual(world.state()["root_results"], {"1": 3})
        world.exit_thread(2, 1)
        self.assertTrue(world.state()["complete"])

    def test_group_exit_clears_thread_tid_once_and_preserves_shared_backing(self):
        world = World()
        world.launch(1, "app.wasm")
        world.map(1, 64, 4, shared=True)
        fork = world.fork(1, 2)
        world.unwind(fork)
        clone = world.clone(1, 1, 7, clear_tid=64)
        world.unwind(clone)
        world.terminate(1, force=True)
        self.assertEqual(world.read(2, 64, 4)["data_hex"], "00000000")
        wakes = [event for event in world.events if event["op"] == "clear_tid.futex_wake"]
        self.assertEqual(len(wakes), 1)


class CliTests(unittest.TestCase):
    def cli(self, *args, text=None):
        return subprocess.run([sys.executable, "-B", "-m", "lind_demo", *args],
                              input=text, capture_output=True, text=True, timeout=15)

    def test_export_custom_file_and_json_trace(self):
        exported = self.cli("export", "cross-runtime-exec")
        self.assertEqual(exported.returncode, 0, exported.stderr)
        replayed = self.cli("file", "-", "--json", text=exported.stdout)
        self.assertEqual(replayed.returncode, 0, replayed.stderr)
        trace = [json.loads(line) for line in replayed.stdout.splitlines()]
        self.assertTrue(any(e["op"] == "ApplyStagingCage" for e in trace))
        self.assertEqual([e["n"] for e in trace], list(range(1, len(trace) + 1)))

    def test_cli_input_failures_return_nonzero_without_traceback(self):
        for text in ['{', '{}', '[{"op":"launch"}]', '[{"op":"not-a-command"}]']:
            with self.subTest(text=text):
                result = self.cli("file", "-", text=text)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("ERROR", result.stderr)
                self.assertNotIn("Traceback", result.stderr)

    def test_repl_continues_after_error_and_keeps_state(self):
        result = self.cli("repl", text='\n'.join([
            '{"op":"launch","cage":1,"image":"app.wasm"}',
            '{"op":"missing"}', "state", "quit", "",
        ]))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ERROR INPUT", result.stderr)
        self.assertIn('"live_threads": 1', result.stdout)


if __name__ == "__main__":
    unittest.main()
