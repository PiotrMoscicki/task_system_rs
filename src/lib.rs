mod tasks {
    use std::{
        sync::{Arc, Mutex, mpsc},
        marker::Send,
    };

    use threadpool::ThreadPool;

    // *********************************************************************************************
    #[derive(Debug, PartialEq, Eq, Copy, Clone)]
    pub enum TaskStatus {
        None,
        Waiting,
        Queued,
        Running,
        Completed,
    }

    // *********************************************************************************************
    #[derive(Debug, PartialEq, Eq, Copy, Clone)]
    pub enum GetValueError {
        NotReady,
        AlreadyTaken,
    }

    // *********************************************************************************************
    trait TaskBase {
        fn status(&self) -> TaskStatus;
        fn queued(&self) -> bool;
        fn running(&self) -> bool;
        fn completed(&self) -> bool;

        fn wait(&mut self);
    }

    // *********************************************************************************************
    struct TaskSharedState<O> {
        status: TaskStatus,
        output: Option<O>,
    }
    
    impl<O> TaskSharedState<O> {
        fn new() -> Self {
            return Self{ 
                status: TaskStatus::None,
                output: None,
            };
        }
    }

    // *********************************************************************************************
    struct Task<O> {
        shared_state: Arc<Mutex<TaskSharedState<O>>>,
    }

    impl<O> Task<O> {
        fn new() -> Self {
            return Self{ 
                shared_state: Arc::new(Mutex::new(TaskSharedState::new())),
            };
        }

        pub fn value(&mut self) -> Result<O, GetValueError> {
            let mut mutex = self.shared_state.lock().unwrap();

            match mutex.status {
                TaskStatus::Completed => {
                    match mutex.output.take() {
                        Some(v) => return Ok(v),
                        None => return Err(GetValueError::AlreadyTaken),
                    }
                },
                _ => return Err(GetValueError::NotReady),
            }
        }
    }

    impl<O> TaskBase for Task<O> {
        fn status(&self) -> TaskStatus {
            let shared_state = self.shared_state.lock().unwrap();
            return shared_state.status;
        }

        fn queued(&self) -> bool {
            let shared_state = self.shared_state.lock().unwrap();
            return shared_state.status == TaskStatus::Queued;
        }

        fn running(&self) -> bool {
            let shared_state = self.shared_state.lock().unwrap();
            return shared_state.status == TaskStatus::Running;
        }

        fn completed(&self) -> bool {
            let shared_state = self.shared_state.lock().unwrap();
            return shared_state.status == TaskStatus::Completed;
        }

        fn wait(&mut self) {
            {
                loop {
                    let mutex = self.shared_state.lock().unwrap();
                    if mutex.status == TaskStatus::Completed {
                        break;
                    }
                }
            }
        }
    }

    // *********************************************************************************************
    struct TaskSystem {
        pool: ThreadPool,
    }

    impl TaskSystem {
        pub fn new(n_workers: usize) -> Self {
            return Self{ pool: ThreadPool::new(n_workers) };
        }
    
        pub fn run<F, O>(&mut self, fun: F) -> Task<O>
            where F: FnOnce() -> O + Send + 'static, O: Send + 'static
        {
            let task = Task::<O>::new();
            
            {
                let mut mutex = task.shared_state.lock().unwrap();
                mutex.status = TaskStatus::Queued;
            }

            let shared_state = task.shared_state.clone();

            self.pool.execute(move || {
                {
                    let mut mutex = shared_state.lock().unwrap();
                    mutex.status = TaskStatus::Running;
                }

                let output = fun();

                {
                    let mut mutex = shared_state.lock().unwrap();
                    mutex.output = Some(output);
                    mutex.status = TaskStatus::Completed;
                }
            });
            
            return task;
        }
    }

    // *********************************************************************************************
    #[cfg(test)]
    mod tests {
        use std::{
            thread,
            time,
            sync::{Barrier},
        };
        use super::*;
    
        #[test]
        fn run_single_task() {
            let mut system = TaskSystem::new(1);

            let mut task = system.run(move|| {
                return 1;
            });

            task.wait();
            assert_eq!(task.status(), TaskStatus::Completed);
            assert_eq!(task.value().unwrap(), 1);
            assert_eq!(task.value(), Err(GetValueError::AlreadyTaken));
        }

        #[test]
        fn run_single_blocking_task() {
            let mut system = TaskSystem::new(1);

            let barrier = Arc::new(Barrier::new(2));
            let barrier_clone = barrier.clone();
            let mut task = system.run(move|| {
                barrier_clone.wait();
            });

            thread::sleep(time::Duration::from_secs(1));
            assert_eq!(task.status(), TaskStatus::Running);
            barrier.wait();
            task.wait();
            assert_eq!(task.status(), TaskStatus::Completed);
            assert_eq!(task.value(), Ok(()));
            assert_eq!(task.value(), Err(GetValueError::AlreadyTaken));
        }
    }
}
