import { createHash } from "node:crypto";

/**
 * task_id = sha256(mission_id ":" loop_epoch [":" seq]). Every worker computes the same
 * id for the same mission in the same loop, and the agent contract accepts each id
 * once, so running 2-3 workers cannot double-spend. `seq` lets one loop take more
 * than one action for a mission; workers must agree on it by deterministic planning.
 */
export function taskId(missionId: string, loopEpoch: number, seq = 0): string {
  const input = seq === 0 ? `${missionId}:${loopEpoch}` : `${missionId}:${loopEpoch}:${seq}`;
  return createHash("sha256").update(input).digest("hex");
}
