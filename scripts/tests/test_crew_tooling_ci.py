"""Keep inexpensive staging/delivery CLI tests registered in required gates."""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
COMMAND = "python3 -m unittest discover -s scripts/tests -p 'test_*crew*.py' -v"


class ToolingGateTests(unittest.TestCase):
    def test_just_check_includes_tooling_discovery(self):
        contents = (ROOT / 'Justfile').read_text()
        check = re.search(r'^check: (.*)$', contents, re.MULTILINE)
        self.assertIn('crew-tooling-test', check.group(1).split())
        recipe = re.search(r'^crew-tooling-test:\n((?:[ \t].*\n)+)', contents, re.MULTILINE)
        self.assertIsNotNone(recipe)
        self.assertIn(COMMAND, recipe.group(1))

    def test_required_ci_policy_runs_same_discovery(self):
        contents = (ROOT / '.github/workflows/nuncio-crew-ci.yml').read_text()
        policy = contents.split('  changes:\n', 1)[1].split('\n  desktop-fast:', 1)[0]
        self.assertIn('name: CI Policy', policy)
        self.assertIn('run: ' + COMMAND, policy)


if __name__ == '__main__':
    unittest.main()
