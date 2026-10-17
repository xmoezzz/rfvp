use crate::script::context::ThreadState;
use crate::script::parser::{Parser, Syscall};
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::subsystem::world::GameData;
use crate::vm_runner::VmRunner;
use std::sync::Arc;

fn parser(bytes: Vec<u8>) -> Parser {
    let mut parser = Parser::default();
    parser.buffer = Arc::new(bytes);
    for (id, name, args) in [(0, "ThreadNext", 0), (1, "ThreadExit", 1)] {
        parser.syscalls.insert(
            id,
            Syscall {
                args,
                name: name.into(),
            },
        );
    }
    parser
}

#[test]
fn text_completion_does_not_resurrect_an_exited_coroutine() {
    // Context zero exits the text-preview coroutine, then yields each frame.
    let mut parser = parser(vec![12, 15, 3, 1, 0, 3, 0, 0, 3, 0, 0]);
    let mut runner = VmRunner::new(ThreadManager::new());
    runner.start_main(0);
    runner.thread_manager_mut().thread_start(15, 5);
    runner
        .thread_manager_mut()
        .set_context_status(15, ThreadState::CONTEXT_STATUS_TEXT);
    let mut game = GameData::default();
    game.motion_manager.text_manager.arm_sync_print_wait(0, 15);
    runner.tick(&mut game, &mut parser, 16).unwrap();
    assert_eq!(
        runner.thread_manager().get_context_status(15),
        ThreadState::CONTEXT_STATUS_NONE
    );
    assert!(game
        .motion_manager
        .text_manager
        .collect_completed_sync_print_waiters()
        .is_empty());
    // A completion already in transit cannot turn the cleared PC into runnable code.
    game.thread_wrapper.thread_text_resume(15);
    runner.tick(&mut game, &mut parser, 16).unwrap();
    assert_eq!(
        runner.thread_manager().get_context_status(15),
        ThreadState::CONTEXT_STATUS_NONE
    );
}

#[test]
fn text_completion_only_releases_text_wait() {
    let mut parser = parser(vec![3, 0, 0]);
    let mut runner = VmRunner::new(ThreadManager::new());
    let mut game = GameData::default();
    for (id, state) in [
        (1, ThreadState::CONTEXT_STATUS_TEXT),
        (2, ThreadState::CONTEXT_STATUS_SLEEP),
    ] {
        runner.thread_manager_mut().thread_start(id, 0);
        runner.thread_manager_mut().set_context_status(id, state);
        runner
            .thread_manager_mut()
            .set_context_sleeping_time(id, 100);
        game.thread_wrapper.thread_text_resume(id);
    }
    runner.tick(&mut game, &mut parser, 16).unwrap();
    assert_eq!(runner.thread_manager().contexts[1].get_pc(), 3);
    assert_eq!(
        runner.thread_manager().get_context_status(2),
        ThreadState::CONTEXT_STATUS_SLEEP
    );
    assert_eq!(runner.thread_manager().contexts[2].get_pc(), 0);
}

#[test]
fn coroutine_restart_cancels_previous_text_waiters() {
    let mut parser = parser(vec![3, 0, 0]);
    let mut runner = VmRunner::new(ThreadManager::new());
    runner.start_main(0);
    let mut game = GameData::default();
    game.motion_manager.text_manager.arm_sync_print_wait(0, 15);
    game.motion_manager.text_manager.arm_sync_print_wait(1, 16);
    game.thread_wrapper.thread_start(15, 0);
    runner.tick(&mut game, &mut parser, 16).unwrap();
    assert_eq!(
        game.motion_manager
            .text_manager
            .collect_completed_sync_print_waiters(),
        vec![16]
    );
}
