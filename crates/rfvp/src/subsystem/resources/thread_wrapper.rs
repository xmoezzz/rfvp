use std::collections::VecDeque;

pub enum ThreadRequest {
    /// start a new thread with the given id and address
    Start(u32, u32),
    /// wait for the given time
    Wait(u32),
    /// wait until dissolve is completed / static
    DissolveWait(),
    /// sleep for the given time, depercated request
    Sleep(u32),
    /// raise the threads which are waiting for the given time, depercated request
    Raise(u32),
    /// yield the current thread
    Next(),
    /// exit the corresponding thread, None is for all threads,
    /// If all threads are exited, the game will be impossible to manipulate through the script engine
    Exit(Option<u32>),
    ShouldBreak(),
}

#[derive(Default)]
pub struct ThreadWrapper {
    requests: VecDeque<ThreadRequest>,
}


impl ThreadWrapper {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn pop(&mut self) -> Option<ThreadRequest> {
        self.requests.pop_front()
    }

    pub fn thread_start(&mut self, id: u32, addr: u32) {
        self.requests.push_back(ThreadRequest::Start(id, addr));
    }

    pub fn thread_wait(&mut self, time: u32) {
        self.requests.push_back(ThreadRequest::Wait(time));
    }

    pub fn dissolve_wait(&mut self) {
        self.requests.push_back(ThreadRequest::DissolveWait());
    }

    pub fn thread_sleep(&mut self, time: u32) {
        self.requests.push_back(ThreadRequest::Sleep(time));
    }

    pub fn thread_raise(&mut self, time: u32) {
        self.requests.push_back(ThreadRequest::Raise(time));
    }

    pub fn thread_next(&mut self) {
        self.requests.push_back(ThreadRequest::Next());
    }

    pub fn thread_exit(&mut self, id: Option<u32>) {
        self.requests.push_back(ThreadRequest::Exit(id));
    }

    pub fn should_break(&mut self) {
        self.requests.push_back(ThreadRequest::ShouldBreak());
    }
}