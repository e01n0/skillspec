#!/usr/bin/env python3
"""SkillOpt EnvAdapter for skillspec — routes LLM calls through the hosting agent.

This adapter connects SkillOpt's training loop to skillspec's test harness.
It uses the agent_proxy model backend, so all LLM calls (rollout, reflection,
ranking) go through stdout/stdin to the hosting agent.

Usage:
    python skillspec_adapter.py --config config.yaml --output-dir out/ --proxy-mode
"""

import json
import os
import sys
import argparse
from pathlib import Path

# Ensure SkillOpt is importable
SKILLOPT_DIR = Path(__file__).parent / "skillopt"
if SKILLOPT_DIR.exists():
    sys.path.insert(0, str(SKILLOPT_DIR))

from skillopt.envs.base import EnvAdapter
from skillopt.model import set_backend, chat_optimizer, chat_target


class SkillSpecAdapter(EnvAdapter):
    """Bridges SkillOpt's training loop with skillspec's test cases."""

    def __init__(self):
        self._skill_name = "unknown"
        self._split_dir = Path(".")
        self._items_cache: dict[str, list[dict]] = {}

    def setup(self, cfg: dict) -> None:
        super().setup(cfg)
        self._skill_name = cfg.get("skill_name", "unknown")
        self._split_dir = Path(cfg.get("split_dir", "."))

        # Activate agent_proxy backend
        set_backend("agent_proxy")

        # Emit init status
        status = {
            "type": "status",
            "phase": "init",
            "message": f"SkillOpt initialized for '{self._skill_name}' with agent_proxy backend",
        }
        print(json.dumps(status), flush=True)

    def _load_items(self, split: str) -> list[dict]:
        if split not in self._items_cache:
            items_path = self._split_dir / split / "items.json"
            if not items_path.exists():
                raise FileNotFoundError(f"Split data not found: {items_path}")
            with open(items_path) as f:
                self._items_cache[split] = json.load(f)
        return self._items_cache[split]

    def build_train_env(self, batch_size: int, seed: int, **kwargs):
        items = self._load_items("train")
        return ItemBatchManager(items, batch_size, seed)

    def build_eval_env(self, env_num: int, split: str, seed: int, **kwargs):
        split_name = split if split else "val"
        items = self._load_items(split_name)
        return ItemBatchManager(items, env_num, seed)

    def rollout(self, env_manager, skill_content: str, out_dir: str, **kwargs) -> list[dict]:
        """Execute skill against each test item via the hosting agent."""
        results = []
        batch = env_manager.get_batch()

        for item in batch:
            # Target model executes the skill
            response_text, _ = chat_target(
                system=skill_content,
                user=json.dumps(item["input"]),
                stage="rollout",
            )

            # Optimizer model scores the response against assertions
            score_prompt = json.dumps({
                "task": "Score this skill output against the test assertions.",
                "response": response_text,
                "expected": item["expected_output"],
                "test_id": item["id"],
                "instructions": (
                    "Return a JSON object with:\n"
                    '  "hard": 1 if ALL assertions pass, 0 otherwise\n'
                    '  "soft": fraction of assertions that pass (0.0 to 1.0)\n'
                    '  "fail_reason": explanation if hard=0, empty string if hard=1'
                ),
            })

            score_text, _ = chat_optimizer(
                system="You evaluate skill outputs against test assertions. Return only valid JSON.",
                user=score_prompt,
                stage="evaluate",
            )

            try:
                scores = json.loads(score_text)
            except json.JSONDecodeError:
                scores = {"hard": 0, "soft": 0.0, "fail_reason": f"Score parse error: {score_text[:200]}"}

            results.append({
                "id": item["id"],
                "hard": int(scores.get("hard", 0)),
                "soft": float(scores.get("soft", 0.0)),
                "n_turns": 1,
                "fail_reason": scores.get("fail_reason", ""),
                "task_type": item.get("task_type", self._skill_name),
                "trace": [
                    {"role": "system", "content": skill_content[:500] + "..."},
                    {"role": "user", "content": json.dumps(item["input"])},
                    {"role": "assistant", "content": response_text},
                ],
            })

        return results

    def reflect(self, results: list[dict], skill_content: str, out_dir: str, **kwargs) -> list[dict | None]:
        """Let SkillOpt's built-in reflection handle this via the proxy backend."""
        return [None]

    def get_task_types(self) -> list[str]:
        return [self._skill_name]


class ItemBatchManager:
    """Serves test items as batches for SkillOpt's training loop."""

    def __init__(self, items: list[dict], batch_size: int, seed: int):
        self.items = list(items)
        self.batch_size = batch_size
        self.seed = seed
        self._offset = 0

        # Deterministic shuffle
        import random
        rng = random.Random(seed)
        rng.shuffle(self.items)

    def get_batch(self) -> list[dict]:
        batch = self.items[self._offset:self._offset + self.batch_size]
        self._offset += self.batch_size
        if self._offset >= len(self.items):
            self._offset = 0
        return batch

    def reset(self):
        self._offset = 0

    def __len__(self):
        return len(self.items)


def main():
    parser = argparse.ArgumentParser(description="SkillSpec SkillOpt adapter")
    parser.add_argument("--config", required=True, help="Path to config YAML")
    parser.add_argument("--output-dir", required=True, help="Output directory")
    parser.add_argument("--proxy-mode", action="store_true", help="Use agent proxy backend")
    parser.add_argument("--resume", help="Resume from state file")
    parser.add_argument("--response", help="JSON response to feed before resuming")
    parser.add_argument("--pipe", help="FIFO path for reading agent responses")
    args = parser.parse_args()

    import yaml
    with open(args.config) as f:
        config = yaml.safe_load(f)

    # If a response was provided, feed it to stdin before the trainer reads
    if args.response:
        # The response needs to be available on stdin when the backend reads it.
        # We write it to a temp file and redirect stdin.
        import tempfile
        with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as tmp:
            tmp.write(args.response + "\n")
            tmp_path = tmp.name
        sys.stdin = open(tmp_path, 'r')

    # Configure FIFO pipe for agent responses
    if args.pipe:
        from skillopt.model.agent_proxy import set_pipe_path
        set_pipe_path(args.pipe)

    adapter = SkillSpecAdapter()

    try:
        from skillopt.engine.trainer import ReflACTTrainer
        from skillopt.config import load_config, flatten_config

        trainer_config = flatten_config(load_config(args.config, {}))
        trainer = ReflACTTrainer(trainer_config, adapter)

        if args.resume and Path(args.resume).exists():
            trainer.resume(args.resume)

        trainer.train()

        # Emit completion
        best_path = Path(args.output_dir) / "best_skill.md"
        best_skill = best_path.read_text() if best_path.exists() else ""

        result = {
            "type": "complete",
            "best_skill": best_skill,
            "score": getattr(trainer, "best_score", None),
            "steps_run": getattr(trainer, "total_steps", None),
        }
        print(json.dumps(result), flush=True)

    except Exception as e:
        error = {"type": "error", "message": str(e)}
        print(json.dumps(error), flush=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
