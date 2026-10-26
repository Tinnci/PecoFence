"""Determinism, checked-in fixture parity, and approved delivery dates."""
import json
import unittest
import board_demo_fixtures as demo


class DeliveryFixtures(unittest.TestCase):
    def test_reproducible_and_checked_in(self):
        first = demo.build()
        self.assertEqual(first, demo.build())
        for name, value in first.items():
            self.assertEqual(value, json.loads((demo.OUT / name).read_text()))

    def test_approved_dates_and_evidence_pointers(self):
        for step in range(3):
            board = demo.snapshot(step)
            cards = {c["business_number"]: c for c in board["cards"]}
            for number, due, baseline in [("103", "10-30", "10-28"), ("124", "10-27", "10-24")]:
                schedule = cards["ORION-R-" + number]["schedule"]
                self.assertEqual(schedule["due"][5:10], due)
                self.assertEqual(schedule["baseline_due"][5:10], baseline)
            blocker = cards["ORION-R-093"]["blockers"][0]
            self.assertEqual(blocker["since"][5:10], "10-18")
            self.assertEqual(blocker["expected_at"][5:10], "11-08")
            for number in ["080", "081", "090", "110", "111"]:
                self.assertEqual(cards["ORION-R-" + number]["stage"]["stage"], "done")
            evidence = {e["card_id"]: {i["id"] for i in e["items"]} for e in demo.card_evidence(board)}
            for c in cards.values():
                self.assertLessEqual(len(c["event_marks"]), 12)
                self.assertEqual(c["stage_history"][-1]["stage"], c["stage"]["stage"])
                self.assertTrue(all(m["evidence_id"] in evidence[c["card_id"]] for m in c["event_marks"]))


if __name__ == "__main__":
    unittest.main()
