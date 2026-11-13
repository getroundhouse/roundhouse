use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

use TaskKind::{Command, Monitor, Subagent, Wait};

fn tasks(kinds: &[TaskKind]) -> Vec<AgentTask> {
    let task = |(at, kind): (usize, &TaskKind)| AgentTask {
        id: at.to_string().as_str().into(),
        description: None,
        kind: *kind,
        call: None,
    };
    kinds.iter().enumerate().map(task).collect()
}

fn opened(groups: &[TaskGroup]) -> Folds {
    let mut folds = Folds::default();
    for group in groups {
        folds.choose(group.key(), Fold::Tasks, true);
    }
    folds
}

#[test]
fn as_many_tasks_as_there_s_room_for_have_a_line_each_in_the_order_they_started() {
    let few = tasks(&[Command, Subagent, Monitor, Subagent]);

    let lines = task_lines(&few, &opened(&[TaskGroup::Subagents]));

    let each = [
        TaskLine::Task(TaskGroup::Commands, &few[0]),
        TaskLine::Task(TaskGroup::Subagents, &few[1]),
        TaskLine::Task(TaskGroup::Monitors, &few[2]),
        TaskLine::Task(TaskGroup::Subagents, &few[3]),
    ];
    assert_eq!(lines, each);
}

#[test]
fn more_are_listed_by_group_with_one_line_for_a_group_with_several() {
    let many = tasks(&[Command, Subagent, Monitor, Subagent, Command, Subagent]);
    let of = |at: &[usize]| at.iter().map(|at| &many[*at]).collect::<Vec<_>>();

    let lines = task_lines(&many, &Folds::default());

    assert_eq!(
        lines,
        [
            TaskLine::Several { group: TaskGroup::Subagents, tasks: of(&[1, 3, 5]), open: false },
            TaskLine::Several { group: TaskGroup::Commands, tasks: of(&[0, 4]), open: false },
            TaskLine::Task(TaskGroup::Monitors, &many[2]),
        ]
    );
}

#[test]
fn an_open_group_has_its_tasks_lines_under_its_own() {
    let many = tasks(&[Command, Subagent, Monitor, Subagent, Command, Subagent]);
    let of = |at: &[usize]| at.iter().map(|at| &many[*at]).collect::<Vec<_>>();

    let lines = task_lines(&many, &opened(&[TaskGroup::Commands]));

    assert_eq!(
        lines,
        [
            TaskLine::Several { group: TaskGroup::Subagents, tasks: of(&[1, 3, 5]), open: false },
            TaskLine::Several { group: TaskGroup::Commands, tasks: of(&[0, 4]), open: true },
            TaskLine::Under(TaskGroup::Commands, &many[0]),
            TaskLine::Under(TaskGroup::Commands, &many[4]),
            TaskLine::Task(TaskGroup::Monitors, &many[2]),
        ]
    );
}

/// The hub keeps the wait, and its line is drawn from the hub's, so the task the agent's CLI lists for it has none,
/// and takes no room from the others.
#[test]
fn a_wait_on_the_pull_request_has_no_line_and_takes_no_room() {
    let with_a_wait = tasks(&[Command, Wait, Command, Command, Command]);

    let lines = task_lines(&with_a_wait, &Folds::default());

    let each: Vec<_> = [0, 2, 3, 4].map(|at| TaskLine::Task(TaskGroup::Commands, &with_a_wait[at])).into();
    assert_eq!(lines, each);
}

#[test]
fn a_group_whose_line_goes_and_comes_back_starts_closed() {
    let many = tasks(&[Subagent, Subagent, Subagent, Command, Command]);
    let mut folds = opened(&[TaskGroup::Subagents, TaskGroup::Commands]);
    let is_open = |folds: &Folds, group: TaskGroup| folds.is_open(&group.key(), Fold::Tasks);

    forget_gone(&many, &mut folds);
    assert!(is_open(&folds, TaskGroup::Subagents) && is_open(&folds, TaskGroup::Commands), "both still have a line");

    // One subagent left, so it has a line of its own, and with as few as there's room for, so do the commands.
    forget_gone(&tasks(&[Subagent, Command, Command]), &mut folds);
    forget_gone(&many, &mut folds);

    assert!(!is_open(&folds, TaskGroup::Subagents) && !is_open(&folds, TaskGroup::Commands));
}

fn kind() -> impl Strategy<Value = TaskKind> {
    prop_oneof![Just(Subagent), Just(Command), Just(Monitor), Just(Wait)]
}

fn open_groups() -> impl Strategy<Value = Vec<TaskGroup>> {
    proptest::sample::subsequence(TaskGroup::ALL.to_vec(), 0..=TaskGroup::ALL.len())
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// Whatever is running and whatever is open, every task but a wait is reached by exactly one line: its own, or
    /// its group's.
    #[test]
    fn every_task_but_a_wait_is_in_exactly_one_line(kinds in vec(kind(), 0..40), open in open_groups()) {
        let tasks = tasks(&kinds);

        let lines = task_lines(&tasks, &opened(&open));

        let mut reached: Vec<&AgentTask> = Vec::new();
        for line in &lines {
            match line {
                TaskLine::Task(_, task) => reached.push(task),
                TaskLine::Several { tasks, .. } => reached.extend(tasks),
                TaskLine::Under(..) => {}
            }
        }
        reached.sort_by_key(|task| task.id.as_str().parse::<usize>().unwrap());
        let with_lines: Vec<&AgentTask> = tasks.iter().filter(|task| task.kind != Wait).collect();
        prop_assert_eq!(reached, with_lines);
    }

    /// The lines under an open group are its tasks' own, in order, and a closed one has none.
    #[test]
    fn only_an_open_group_has_lines_under_it_one_for_each_task(kinds in vec(kind(), 0..40), open in open_groups()) {
        let tasks = tasks(&kinds);

        let lines = task_lines(&tasks, &opened(&open));

        let mut expected = Vec::new();
        for line in &lines {
            if let TaskLine::Several { group, tasks, open: true } = line {
                expected.extend(tasks.iter().map(|task| (*group, *task)));
            }
        }
        let under = lines.iter().filter_map(|line| match line {
            TaskLine::Under(group, task) => Some((*group, *task)),
            TaskLine::Task(..) | TaskLine::Several { .. } => None,
        });
        prop_assert_eq!(under.collect::<Vec<_>>(), expected);
    }

    /// With nothing open, there are never more lines than there's room for or than there are groups.
    #[test]
    fn closed_the_lines_are_no_more_than_theres_room_for(kinds in vec(kind(), 0..40)) {
        let tasks = tasks(&kinds);

        let lines = task_lines(&tasks, &Folds::default());

        prop_assert!(lines.len() <= ROOM.max(TaskGroup::ALL.len()));
    }
}
