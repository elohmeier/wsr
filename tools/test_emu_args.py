"""Regression checks for exact numeric arguments to the original-executable oracle.

Run after mise run setup: .venv/bin/python tools/test_emu_args.py
No original game files are needed for these argument-codec checks.
"""
from pathlib import Path
import struct
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent / 'emu'))
import game


class NumericArguments(unittest.TestCase):
    def test_extended_arguments_preserve_low_bits_range_and_signed_zero(self):
        cases = [
            (0x3fff, 0x8000000000000001),  # 1 + 2**-63, rounded away by f64
            (0x0000, 1),                   # Below DOUBLE's exponent range
            (0x7ffe, 0xffffffffffffffff),  # Above DOUBLE's exponent range
            (0x8000, 0),                   # Negative zero
        ]
        for sign_exponent, mantissa in cases:
            with self.subTest(sign_exponent=sign_exponent, mantissa=mantissa):
                value = f'x87:{sign_exponent:04x}{mantissa:016x}'
                kind, parsed = game.parse_arg('e:' + value)
                self.assertEqual(kind, 'e')
                self.assertEqual(parsed, value)
                self.assertEqual(game.encode_ext(parsed), struct.pack('<QH', mantissa, sign_exponent))
        exact = game.encode_ext(game.parse_arg('e:x87:3fff8000000000000001')[1])
        self.assertNotEqual(exact, game.encode_ext(1.0))

    def test_legacy_decimal_arguments_keep_their_existing_types(self):
        for kind in ('e', 'd', 'f'):
            self.assertEqual(game.parse_arg(kind + ':1.25'), (kind, 1.25))

    def test_malformed_extended_arguments_are_rejected(self):
        for value in ('x87:', 'x87:3fff800000000000000', 'x87:zzzz8000000000000001'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                game.parse_arg('e:' + value)


if __name__ == '__main__':
    unittest.main()
