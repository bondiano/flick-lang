/** Pure transport projection shared by server rendering and browser hydration. */
import type { ExerciseMetadata } from "../../exercises/load.ts";

export const lessonConfiguration = (exercise: ExerciseMetadata | undefined) => ({
  lesson_title: exercise?.title ?? "",
  lesson_title_ru: exercise?.title_ru ?? "",
  lesson_task: exercise?.task ?? "",
  lesson_task_ru: exercise?.task_ru ?? "",
  lesson_hints: [...exercise?.hints ?? []],
  lesson_hints_ru: [...exercise?.hints_ru ?? []],
  lesson_question: exercise?.question?.text ?? "",
  lesson_question_ru: exercise?.question?.text_ru ?? "",
  lesson_choices: [...exercise?.question?.choices ?? []],
  lesson_choices_ru: [...exercise?.question?.choices_ru ?? []],
  // the right choice, or -1 without a question
  lesson_answer: exercise?.question?.answer ?? -1,
});
