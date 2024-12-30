pub mod threei;
pub mod threeiconstant;
pub mod syscall_table;
pub mod syscall_map;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threei::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn test_make_syscall() {
        {
            let mut handler_table = HANDLERTABLE.lock().unwrap();
            let mut cage_call_table = CageCallTable::new();
            cage_call_table.register_handler(1, threeiconstant::THREEI_MATCHALL); 

            handler_table.insert(1, Arc::new(Mutex::new(cage_call_table))); // targetcage = 1
        }

        // test make
        let result = make_syscall(
            1, // syscall num
            1, // targetcage
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        );

        assert_eq!(result, 0, "Syscall did not return the expected result");
    }
}
