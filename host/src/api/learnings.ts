/**
 * Learnings API - typed wrapper around os learning commands
 *
 * Note: Learnings are now added via tasks.complete() with the learnings option.
 * They bubble to immediate parent on completion.
 * This API only provides read access for viewing learnings.
 */
import { callCli } from "../cli.js";
import { decodeLearnings } from "../decoder.js";
import type { Learning } from "../types.js";

/**
 * Learnings API exposed to VM sandbox
 */
export const learnings = {
  /**
   * List all learnings for a task.
   * Includes learnings bubbled from completed child tasks.
   */
  async list(taskId: string): Promise<Learning[]> {
    return decodeLearnings(await callCli(["learning", "list", taskId])).unwrap("learnings.list");
  },
};
