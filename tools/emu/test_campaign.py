"""Pure comparator tests, plus explicit --integration tests using the owner's original executable."""
import contextlib
import datetime as dt
import io
import json
import math
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import campaign

INTEGRATION = '--integration' in sys.argv
if INTEGRATION:
    sys.argv.remove('--integration')


class Comparison(unittest.TestCase):
    def test_integer_and_currency_values_remain_exact_above_2_to_53(self):
        layout = [('a', 'CashRaw', 'CUR', None), ('b', 'Counter', 'QUAD', None)]
        count, _ = campaign.differences({'a': 2**60, 'b': 2**60}, {'a': 2**60+1, 'b': 2**60+1}, layout)
        self.assertEqual(count, 2)

    def test_numeric_tolerance_does_not_hide_strings_or_new_nonfinite_cells(self):
        layout = [('a', 'Price', 'EXT', None), ('b', 'Options', 'STRING', None)]
        self.assertEqual(campaign.differences({'a': 1.0, 'b': 'x'}, {'a': 1+1e-12, 'b': 'x'}, layout)[0], 0)
        self.assertEqual(campaign.differences({'a': 1.0, 'b': 'x'}, {'a': 1+1e-12, 'b': 'y'}, layout)[0], 1)
        self.assertEqual(campaign.differences({'a': None, 'b': 'x'}, {'a': None, 'b': 'x'}, layout)[0], 1)
        self.assertEqual(campaign.differences({'a': None, 'b': 'x'}, {'a': None, 'b': 'x'}, layout, allowed_nonfinite={('a', 0)})[0], 0)
        self.assertEqual(campaign.differences({'a': float('inf'), 'b': 'x'}, {'a': float('inf'), 'b': 'x'}, layout)[0], 1)

    def test_missing_fields_and_equal_truncated_arrays_fail(self):
        layout = [('a', 'History', 'EXT', [(1, 3), (0, 1)])]
        self.assertEqual(campaign.differences({}, {}, layout)[0], 1)
        self.assertEqual(campaign.differences({'a': [0]}, {'a': [0]}, layout)[0], 1)
        _, result = campaign.differences({'a': [0]*6}, {'a': [0, 0, 0, 0, 0, 1]}, layout)
        self.assertEqual(result[0]['index'], [3, 1])

    def test_preexisting_nonfinite_exemption_cannot_expand(self):
        layout = [('a', 'History', 'EXT', [(1, 3)])]
        baseline = campaign.initial_nonfinite({'a': [None, 0, 0]}, layout)
        self.assertEqual(campaign.differences({'a': [None, None, 0]}, {'a': [None, None, 0]}, layout, allowed_nonfinite=baseline)[0], 1)
        retired = campaign.retained_nonfinite(baseline, {'a': [1, 0, 0]}, {'a': [1, 0, 0]})
        self.assertFalse(retired)
        self.assertEqual(campaign.differences({'a': [None, 0, 0]}, {'a': [None, 0, 0]}, layout, allowed_nonfinite=retired)[0], 1)

    def test_extended_snapshots_keep_low_bits_and_values_outside_double_range(self):
        layout = [('a', 'Amount', 'EXT', None)]
        low = 'x87:3fff8000000000000001'
        self.assertEqual(campaign.differences({'a': low}, {'a': 1.0}, layout)[0], 0)
        self.assertEqual(campaign.differences({'a': low}, {'a': 1.0}, layout, exact_ext=True)[0], 1)
        wide = 'x87:60008000000000000000'
        self.assertTrue(campaign.finite(wide))
        self.assertEqual(campaign.differences({'a': wide}, {'a': wide}, layout, exact_ext=True)[0], 0)
        self.assertEqual(campaign.differences({'a': 0.0}, {'a': -0.0}, layout, exact_ext=True)[0], 1)

    def test_extended_nonfinite_values_still_require_an_explicit_input_exemption(self):
        layout = [('a', 'Amount', 'EXT', None)]
        state = {'a': 'x87:ffffc000000000000000'}
        self.assertEqual(campaign.differences(state, state, layout)[0], 1)
        allowed = campaign.initial_nonfinite(state, layout)
        self.assertEqual(campaign.differences(state, state, layout, allowed_nonfinite=allowed)[0], 0)

    def test_cp1252_protocol_keeps_the_original_bytes(self):
        self.assertEqual(campaign.byte_text('£€'), '\xa3\x80')

    def test_invalid_and_ambiguous_plans_fail(self):
        for step in [{'op': 'unknown'}, {'op': 'tick', 'repeat': 0}, {'op': 'tick', 'repeat': True},
                     {'op': 'tick', 'skips': []}, {'op': 'clock', 'arg': True}, {'op': 'trade', 'mode': 4, 'arg': 0}]:
            with self.assertRaises(ValueError):
                list(campaign.expand_steps([step]))
        self.assertEqual(len(list(campaign.expand_steps([{'op': 'tick', 'repeat': 3}]))), 3)

    def test_reports_ignore_only_window_padding(self):
        a = [{'kind': 'report', 'title': 'Report', 'lines': ['ABC  ', ' ', '']}]
        b = [{'kind': 'report', 'title': 'Report', 'lines': ['ABC  ']}]
        self.assertFalse(campaign.ui_differences(a, b))
        self.assertTrue(campaign.ui_differences(a, [{'kind': 'report', 'title': 'Report', 'lines': ['XYZ']}]))

    def test_file_bytes_are_exact_and_deletion_is_distinct_from_no_write(self):
        self.assertFalse(campaign.file_differences({'TMP_QM.DAT': b'a'}, {'TMP_QM.DAT': b'b'}))
        self.assertEqual(len(campaign.file_differences({'GAME01.DAT': None}, {})), 1)
        diff = campaign.file_differences({'GAME01.DAT': b'1.00000000001'}, {'GAME01.DAT': b'1.00000000002'})
        self.assertEqual(diff[0]['first_byte_difference'], 12)


class Integration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not INTEGRATION:
            raise unittest.SkipTest('Pass --integration to require the original executable and Rust driver')
        import game
        cls.game = game
        if not Path(game.DEFAULT_EXE).is_file():
            raise RuntimeError('Integration explicitly requested but original WSR.EXE is missing')
        cls.temp = tempfile.TemporaryDirectory(prefix='wsr-campaign-tests-')
        cls.root = Path(cls.temp.name)
        cls.assets = cls.root / 'assets'
        cls.assets.mkdir()
        cls.initial = cls.root / 'initial.json'
        # A complete finite default snapshot tests the protocol without bundling any game data.
        state = {}
        for key, _, typ, bounds in game.layout():
            value = '' if typ == 'STRING' else 0
            state[key] = [value] * math.prod(hi-lo+1 for lo, hi in bounds) if bounds else value
        campaign.dump(cls.initial, state)
        import subprocess
        subprocess.run(['cargo', 'build', '--manifest-path', str(campaign.ROOT / 'engine/Cargo.toml'), '-p', 'wsr-wasm', '--example', 'campaign_driver', '--profile', 'test'], check=True)
        cls.driver = campaign.ROOT / 'engine/target/debug/examples/campaign_driver'

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def run_case(self, name, steps, initial=None, extra=()):
        plan = self.root / (name + '-plan.json')
        output = self.root / name
        campaign.dump(plan, {'version': 1, 'steps': steps})
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            code = campaign.main(['--snapshot', str(initial or self.initial), '--assets', str(self.assets), '--driver', str(self.driver), '--plan', str(plan), '--out', str(output), *extra])
        return code, json.loads((output / 'result.json').read_text()), output

    def test_clock_and_persistent_peers_match_and_replay(self):
        code, result, out = self.run_case('clock', [{'op': 'pause', 'arg': 1, 'repeat': 3}, {'op': 'state'}])
        self.assertEqual((code, result['status'], result['compared_commands']), (0, 'passed', 4))
        state = json.loads((out / 'rust/current.json').read_text())
        ticker_pause = next(key for key, name, _, _ in self.game.layout() if name == 'TickerPauseUntil')
        self.assertEqual(state[ticker_pause], 36007)
        replay = self.root / 'clock-replay'
        with contextlib.redirect_stdout(io.StringIO()):
            code = campaign.main(['--replay', str(out / 'run.json'), '--driver', str(self.driver), '--out', str(replay)])
        self.assertEqual(code, 0)

    def test_first_injected_divergence_is_retained_in_artifacts(self):
        original = campaign.original_command
        calls = 0
        def injected(game, emu, command, now):
            nonlocal calls
            result = original(game, emu, command, now)
            calls += 1
            if calls == 2:
                key = next(key for key, name, _, _ in game.layout() if name == 'TmpExt1')
                emu.write_state({key: 5.0})
            return result
        with patch.object(campaign, 'original_command', injected):
            code, result, out = self.run_case('injected', [{'op': 'state', 'repeat': 3}])
        self.assertEqual((code, result['status'], result['first_divergence'], result['compared_commands']), (1, 'diverged', 2, 1))
        self.assertEqual(result['state_differences'][0]['field'], 'TmpExt1')
        self.assertTrue((out / 'before-rust.json').exists())
        self.assertTrue((out / 'after-original.json').exists())
        self.assertEqual(len((out / 'trace.jsonl').read_text().splitlines()), 2)

    def test_reports_and_cp1252_notices_match_the_original(self):
        code, result, _ = self.run_case('ui', [{'op': 'notice', 'text': '£€\r\nsecond line'}, {'op': 'interest_rates'}])
        self.assertEqual((code, result['status'], result['compared_commands']), (0, 'passed', 2))

    def test_strict_mode_rejects_existing_nonfinite_values(self):
        state = json.loads(self.initial.read_text())
        key = next(key for key, name, _, _ in self.game.layout() if name == 'TmpExt1')
        state[key] = None
        path = self.root / 'invalid.json'
        campaign.dump(path, state)
        code, result, _ = self.run_case('nonfinite', [{'op': 'state'}], path)
        self.assertEqual((code, result['status'], result['compared_commands']), (2, 'invalid-input', 0))

    def test_original_runtime_errors_cannot_pass_as_matching_state(self):
        original = campaign.original_command
        def failed(game, emu, command, now):
            result = original(game, emu, command, now)
            emu.log.append('PBERR 9 in 0x4c6bc1 (test injection)')
            return result
        with patch.object(campaign, 'original_command', failed):
            code, result, _ = self.run_case('runtime-error', [{'op': 'state'}])
        self.assertEqual((code, result['status'], result['compared_commands']), (1, 'runtime-error', 0))

    def test_unreached_campaign_target_is_not_a_pass(self):
        code, result, _ = self.run_case('incomplete', [{'op': 'state', 'repeat': 2}], extra=['--quarters', '1'])
        self.assertEqual((code, result['status'], result['compared_commands']), (1, 'incomplete', 2))

    def test_original_ext_storage_survives_observation_and_multiple_calls(self):
        game = self.game
        emu = campaign.original_peer(game, json.loads(self.initial.read_text()), {}, 30, [])
        key = next(key for key, name, _, _ in game.layout() if name == 'TmpExt1')
        # 1 + 2^-63 is representable in x87 but rounds to 1 in f64. Snapshot observation must
        # not round-trip it through f64 and erase the low bits between campaign commands.
        raw = struct.pack('<QH', (1 << 63) + 1, 16383)
        emu.uc.mem_write(int(key, 16), raw)
        for second in (2, 4):
            error, _ = campaign.original_command(game, emu, {'op': 'state'}, dt.datetime(2000, 1, 3, 10, 0, second))
            self.assertIsNone(error)
            emu.read_state({key}); emu.fingerprint()
            self.assertEqual(emu.rd(int(key, 16), 10), raw)
        self.assertNotEqual(raw, game.f64_to_ext(game.ext_to_f64(raw)))


if __name__ == '__main__':
    unittest.main()
