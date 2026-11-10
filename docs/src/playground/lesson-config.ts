/** Pure transport projection shared by server rendering and browser hydration. */
import type { ExerciseMetadata } from "../../exercises/load.ts";

export const lessonConfiguration = (exercise: ExerciseMetadata | undefined) => ({
  lesson_title: exercise?.title ?? "",
  lesson_title_ru: exercise?.title_ru ?? "",
  lesson_task: exercise?.task ?? "",
  lesson_task_ru: exercise?.task_ru ?? "",
  lesson_hint_text: exercise?.hint ?? "",
  lesson_hint_text_ru: exercise?.hint_ru ?? "",
});
