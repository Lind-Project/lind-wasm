#[cfg(test)]
pub mod vmmap_tests {
    use super::super::*;
    use crate::interface;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem}; 
    use crate::safeposix::syscalls::*;

    // Constants for testing
    // Base address for test memory regions (4KB aligned)
    const TEST_MEM_START: usize = 0x1000;
    // Size of test memory regions (4KB)
    const TEST_MEM_SIZE: usize = 0x1000;
    // Memory protection flags
    const PROT_READ: u32 = 1;   // Read permission
    const PROT_WRITE: u32 = 2;  // Write permission 
    const PROT_EXEC: u32 = 4;   // Execute permission
    const EXIT_SUCCESS: i32 = 0;


    /*
    Test memory persistence after cage exit
    
    Layout:
    Initial:
    Cage A:  [----Memory Region----]
            [Written: "test_data"]
    
    After Exit:
    Cage A:  (cleaned up)
    New Cage: [----Memory Region----]
            [Should be clean]
    */
    #[test]
    pub fn ut_lind_vmmap_memory_persistence() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        // Create initial cage
        let cage = interface::cagetable_getref(1);
        
        // Allocate memory region with read/write permissions
        let mut vmmap = cage.vmmap.write();
        assert_eq!(
            vmmap.map_memory(TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE),
            Ok(()),
            "Failed to map memory"
        );
        
        // Get the mapped region
        let region = vmmap.get_region(TEST_MEM_START)
            .expect("Memory region not found");
        
        // Verify initial mapping
        assert_eq!(region.permissions, PROT_READ | PROT_WRITE);
        assert!(region.contains_range(TEST_MEM_START, TEST_MEM_SIZE));
        
        // Exit the cage
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        
        // Verify cleanup
        assert!(interface::cagetable_lookup(1).is_none(), 
            "Cage still exists after exit");
        
        // Create new cage and try to map same region
        let new_cage = interface::cagetable_getref(2);
        let mut new_vmmap = new_cage.vmmap.write();
        
        // Try to map the same memory region
        assert_eq!(
            new_vmmap.map_memory(TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE),
            Ok(()),
            "Failed to map previously used memory region"
        );
        
        // Verify new mapping is clean
        let new_region = new_vmmap.get_region(TEST_MEM_START)
            .expect("New memory region not found");
        
        assert_eq!(new_region.permissions, PROT_READ | PROT_WRITE);
        assert!(new_region.contains_range(TEST_MEM_START, TEST_MEM_SIZE));
        
        // Cleanup new cage
        assert_eq!(new_cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        
        lindrustfinalize();
    }

    /*
    Test basic memory cleanup after exit
    
    Initial:
    Cage A:  [----Memory Region----]
    
    After Exit:
    Cage A:  (cleaned up)
    */
    #[test]
    fn ut_lind_test_basic_exit_cleanup() {
        let _thelock = setup::lock_and_init();
        
        // Setup cage with memory allocation
        let cage_a = create_test_cage();
        
        // Allocate memory and verify
        setup_memory_region(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Exit cage
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        
        // Verify cleanup
        verify_memory_cleanup(cage_a.cageid);
        lindrustfinalize();
    }

    /*
    Test nested memory regions across multiple cages
    
    Layout:
    Cage A:  [----------------]
    Cage B:    [--------]
    Cage C:      [----]
    */
    #[test]
    fn ut_lind_test_nested_overlaps() {
        let _thelock = setup::lock_and_init();
        
        // Create cages
        let cage_a = create_test_cage();
        let cage_b = create_test_cage();
        let cage_c = create_test_cage();
        
        // Setup nested memory regions
        setup_memory_region(&cage_a, TEST_MEM_START, TEST_MEM_SIZE * 4, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE * 2, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_c, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Verify initial setup
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE * 4, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE * 2, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_c, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Test exits in sequence
        assert_eq!(cage_c.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_c.cageid);
        
        // Verify B and A still accessible
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE * 2, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE * 4, PROT_READ | PROT_WRITE);
        
        assert_eq!(cage_b.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_b.cageid);
        
        // Verify A still accessible
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE * 4, PROT_READ | PROT_WRITE);
        
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_a.cageid);
        lindrustfinalize();
    }

    /*
    Test interleaved memory regions
    
    Layout:
    Cage A:  [---]   [---]   [---]
    Cage B:      [---]   [---]   [---]
    */
    #[test]
    fn ut_lind_test_interleaved_regions() {
        let _thelock = setup::lock_and_init();
        
        let cage_a = create_test_cage();
        let cage_b = create_test_cage();
        
        // Setup interleaved regions for Cage A
        setup_memory_region(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_a, TEST_MEM_START + TEST_MEM_SIZE * 3, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_a, TEST_MEM_START + TEST_MEM_SIZE * 5, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Setup interleaved regions for Cage B
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE * 4, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE * 6, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Verify all regions are accessible
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_a, TEST_MEM_START + TEST_MEM_SIZE * 3, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_a, TEST_MEM_START + TEST_MEM_SIZE * 5, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE * 4, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE * 6, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Test cleanup
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_a.cageid);
        
        // Verify B's regions still accessible
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE * 4, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE * 6, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        assert_eq!(cage_b.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_b.cageid);
        lindrustfinalize();
    }

    /*
    Test circular shared memory dependencies
    
    Layout:
    Cage A → Cage B
        ↑     ↓
        Cage C
    */
    #[test]
    fn ut_lind_test_circular_dependencies() {
        let _thelock = setup::lock_and_init();
        
        let cage_a = create_test_cage();
        let cage_b = create_test_cage();
        let cage_c = create_test_cage();
        
        // Setup circular shared memory regions
        setup_memory_region(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_c, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Verify initial access
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_c, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Test cleanup in different orders
        assert_eq!(cage_b.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_b.cageid);
        
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_c, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        assert_eq!(cage_c.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_c.cageid);
        
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_a.cageid);
        lindrustfinalize();
    }

    /*
    Test splitting shared memory regions
    
    Initial:
    [----------------]
    
    After Split:
    [----]  [----]  [----]
    */
    #[test]
    fn ut_lind_test_split_regions() {
        let _thelock = setup::lock_and_init();
        
        let cage = create_test_cage();
        
        // Setup initial large region
        setup_memory_region(&cage, TEST_MEM_START, TEST_MEM_SIZE * 3, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage, TEST_MEM_START, TEST_MEM_SIZE * 3, PROT_READ | PROT_WRITE);
        
        // Split into three regions with different permissions
        let mut vmmap = cage.vmmap.write();
        vmmap.unmap_memory(TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE);
        
        // Verify split regions
        verify_memory_access(&cage, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        assert!(vmmap.get_region(TEST_MEM_START + TEST_MEM_SIZE).is_none(),
            "Middle region should be unmapped");
        
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage.cageid);
        lindrustfinalize();
    }

    /*
    Test overlapping regions with different permissions
    
    Layout:
    Cage A:  [RW-][R--][RWX]
    Cage B:  [R--][RWX][RW-]
    */
    #[test]
    fn ut_lind_test_mixed_permissions() {
        let _thelock = setup::lock_and_init();
        
        let cage_a = create_test_cage();
        let cage_b = create_test_cage();
        
        // Setup regions with different permissions for Cage A
        setup_memory_region(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_a, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ);
        setup_memory_region(&cage_a, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE | PROT_EXEC);
        
        // Setup regions with different permissions for Cage B
        setup_memory_region(&cage_b, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ);
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE | PROT_EXEC);
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Verify permissions
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_a, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ);
        verify_memory_access(&cage_a, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE | PROT_EXEC);
        
        verify_memory_access(&cage_b, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE | PROT_EXEC);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Test cleanup
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_a.cageid);
        
        assert_eq!(cage_b.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_b.cageid);
        lindrustfinalize();
    }

    /*
    Test memory boundary conditions
    
    Layout:
    Cage A:  [----]
    Cage B:  [----]  (exactly adjacent)
    */
    #[test]
    fn ut_lind_test_boundary_cases() {
        let _thelock = setup::lock_and_init();
        
        let cage_a = create_test_cage();
        let cage_b = create_test_cage();
        
        // Setup adjacent regions
        setup_memory_region(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Verify each region
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Test boundary protection
        let vmmap_a = cage_a.vmmap.read();
        let vmmap_b = cage_b.vmmap.read();
        
        assert!(vmmap_a.get_region(TEST_MEM_START + TEST_MEM_SIZE).is_none(), 
            "Cage A should not access Cage B's memory");
        assert!(vmmap_b.get_region(TEST_MEM_START).is_none(), 
            "Cage B should not access Cage A's memory");
        
        // Test cleanup
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_a.cageid);
        
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        assert_eq!(cage_b.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_b.cageid);
        lindrustfinalize();
    }

    /*
    Test dynamic memory region growth
    
    Initial:
    Cage A:  [----]
    Cage B:      [----]
    
    After Growth:
    Cage A:  [--------]
    Cage B:      [----]
    */
    #[test]
    fn ut_lind_test_dynamic_growth() {
        let _thelock = setup::lock_and_init();
        
        let cage_a = create_test_cage();
        let cage_b = create_test_cage();
        
        // Initial setup
        setup_memory_region(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        setup_memory_region(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Verify initial state
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Grow cage_a's region
        let mut vmmap = cage_a.vmmap.write();
        vmmap.map_memory(TEST_MEM_START, TEST_MEM_SIZE * 2, PROT_READ | PROT_WRITE);
        
        // Verify growth
        verify_memory_access(&cage_a, TEST_MEM_START, TEST_MEM_SIZE * 2, PROT_READ | PROT_WRITE);
        verify_memory_access(&cage_b, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Cleanup
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_a.cageid);
        
        assert_eq!(cage_b.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_b.cageid);
        lindrustfinalize();
    }

    /*
    Test invalid memory operations
    
    Cases:
    1. Zero-sized region
    2. Invalid permissions
    3. Overlapping with invalid permissions
    4. Out of bounds access
    */
    #[test]
    fn ut_lind_test_invalid_memory_operations() {
        let _thelock = setup::lock_and_init();
        
        let cage = create_test_cage();
        
        // Test zero-sized region
        assert!(cage.vmmap.write().map_memory(TEST_MEM_START, 0, PROT_READ).is_err());
        
        // Test invalid permissions
        assert!(cage.vmmap.write().map_memory(TEST_MEM_START, TEST_MEM_SIZE, 0xFF).is_err());
        
        // Test valid setup
        setup_memory_region(&cage, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        
        // Test overlapping with invalid permissions
        assert!(cage.vmmap.write().map_memory(TEST_MEM_START, TEST_MEM_SIZE, 0xFF).is_err());
        
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage.cageid);
        lindrustfinalize();
    }

    /*
    Test concurrent memory access
    
    Layout:
    Multiple cages accessing same memory regions
    */
    #[test]
    fn ut_lind_test_concurrent_access() {
        let _thelock = setup::lock_and_init();
        
        use std::sync::Arc;
        use std::thread;
        
        let cage_count = 5;
        let cages: Vec<Arc<Cage>> = (0..cage_count)
            .map(|_| Arc::new(create_test_cage()))
            .collect();
        
        // Setup shared region
        for cage in &cages {
            setup_memory_region(&cage, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        }
        
        // Concurrent access
        let handles: Vec<_> = cages.iter().map(|cage| {
            let cage = Arc::clone(cage);
            thread::spawn(move || {
                verify_memory_access(&cage, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
            })
        }).collect();
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Cleanup
        for cage in cages {
            assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
            verify_memory_cleanup(cage.cageid);
        }
        lindrustfinalize();
    }

    /*
    Test overlapping regions with conflicting permissions
    
    Layout:
    Initial:
    Cage A:  [RW-][-W-][RWX]
    Cage B:  [R--][--X][RW-]
            |    |    |
            v    v    v
    Result:  [R--][---][RW-]  (most restrictive permissions win)
    */
    #[test]
    fn ut_lind_test_overlapping_permissions_conflict() {
        let _thelock = setup::lock_and_init();
        
        let cage_a = create_test_cage();
        let cage_b = create_test_cage();
        
        // Setup overlapping regions with different permissions
        setup_memory_region(&cage_a, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE).unwrap();
        setup_memory_region(&cage_a, TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE, PROT_WRITE).unwrap();
        setup_memory_region(&cage_a, TEST_MEM_START + TEST_MEM_SIZE * 2, TEST_MEM_SIZE, 
            PROT_READ | PROT_WRITE | PROT_EXEC).unwrap();
        
        // Try to set conflicting permissions
        let result = setup_memory_region(&cage_b, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ);
        assert!(result.is_err(), "Should not allow conflicting permissions");
        
        // Cleanup
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_a.cageid);
        
        assert_eq!(cage_b.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_b.cageid);
        lindrustfinalize();
    }

    /*
    Test maximum number of shared regions
    
    Layout:
    Cage A:  [1][2][3]...[MAX]
    Cage B:  [1][2][3]...[MAX]
            |  |  |      |
            Shared Regions
    */
    #[test]
    fn ut_lind_test_memory_sharing_limits() {
        let _thelock = setup::lock_and_init();
        
        let cage_a = create_test_cage();
        let cage_b = create_test_cage();
        
        const MAX_REGIONS: usize = 10; // Adjust based on system limits
        let mut regions = Vec::new();
        
        // Try to create maximum number of shared regions
        for i in 0..MAX_REGIONS {
            let start = TEST_MEM_START + (i * TEST_MEM_SIZE);
            let result = setup_memory_region(&cage_a, start, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
            if result.is_ok() {
                regions.push(start);
            } else {
                println!("Max regions reached at {}", i);
                break;
            }
        }
        
        // Verify all regions are accessible
        for &start in &regions {
            verify_memory_access(&cage_a, start, TEST_MEM_SIZE, PROT_READ | PROT_WRITE);
        }
        
        // Cleanup
        assert_eq!(cage_a.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_a.cageid);
        
        assert_eq!(cage_b.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        verify_memory_cleanup(cage_b.cageid);
        lindrustfinalize();
    }

    /*
    Test concurrent map/unmap operations
    
    Layout:
    Thread 1:  [Map][Unmap][Map  ]
    Thread 2:  [   Map   ][Unmap ]
    Thread 3:  [  Unmap  ][  Map ]
    Time    -> ------------------>
    */
    #[test]
    fn ut_lind_test_concurrent_mapping_unmapping() {
        let _thelock = setup::lock_and_init();
        
        use std::sync::Arc;
        use std::thread;
        use std::sync::Barrier;
        
        let cage = Arc::new(create_test_cage());
        let barrier = Arc::new(Barrier::new(3));
        
        // Setup initial region
        setup_memory_region(&cage, TEST_MEM_START, TEST_MEM_SIZE, PROT_READ | PROT_WRITE).unwrap();
        
        // Create threads for concurrent operations
        let handles: Vec<_> = (0..3).map(|i| {
            let cage = Arc::clone(&cage);
            let barrier = Arc::clone(&barrier);
            
            thread::spawn(move || {
                barrier.wait();
                match i {
                    0 => {
                        // Thread 1: Map -> Unmap -> Map
                        setup_memory_region(&cage, TEST_MEM_START + TEST_MEM_SIZE, 
                            TEST_MEM_SIZE, PROT_READ | PROT_WRITE).unwrap();
                        cage.vmmap.write().unmap_memory(TEST_MEM_START + TEST_MEM_SIZE, TEST_MEM_SIZE);
                        setup_memory_region(&cage, TEST_MEM_START + TEST_MEM_SIZE * 2, 
                            TEST_MEM_SIZE, PROT_READ | PROT_WRITE).unwrap();
                    },
                    1 => {
                        // Thread 2: Map larger region -> Unmap portion
                        setup_memory_region(&cage, TEST_MEM_START + TEST_MEM_SIZE * 3, 
                            TEST_MEM_SIZE * 2, PROT_READ | PROT_WRITE).unwrap();
                        cage.vmmap.write().unmap_memory(TEST_MEM_START + TEST_MEM_SIZE * 4, TEST_MEM_SIZE);
                    },
                    _ => {
                        // Thread 3: Unmap -> Map different region
                        cage.vmmap.write().unmap_memory(TEST_MEM_START, TEST_MEM_SIZE);
                        setup_memory_region(&cage, TEST_MEM_START + TEST_MEM_SIZE * 5, 
                            TEST_MEM_SIZE, PROT_READ | PROT_WRITE).unwrap();
                    }
                }
            })
        }).collect();
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Verify final state
        let vmmap = cage.vmmap.read();
        assert!(vmmap.get_region(TEST_MEM_START).is_none(), "Region should be unmapped");
        assert!(vmmap.get_region(TEST_MEM_START + TEST_MEM_SIZE * 2).is_some(), 
            "Region should be mapped");
        assert!(vmmap.get_region(TEST_MEM_START + TEST_MEM_SIZE * 3).is_some(), 
            "Region should be mapped");
        assert!(vmmap.get_region(TEST_MEM_START + TEST_MEM_SIZE * 5).is_some(), 
            "Region should be mapped");
        
        // Cleanup
        assert_eq!(Arc::try_unwrap(cage).unwrap().exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    // Helper functions
    fn create_test_cage() -> Cage {
        let cage_id = interface::get_new_cage_id();
        let cage = Cage::new(cage_id);
        interface::cagetable_insert(cage_id, cage.clone());
        cage
    }

    fn verify_memory_cleanup(cage_id: u64) {
        // Check if cage exists in cagetable
        assert!(interface::cagetable_lookup(cage_id).is_none(), 
            "Cage still exists in cagetable after cleanup");
        
        // Verify fd table cleanup
        assert!(fdtables::get_fdtable_for_cage(cage_id).is_none(), 
            "FD table still exists after cleanup");
            
        // Verify all memory regions are unmapped
        assert!(interface::get_memory_regions(cage_id).is_empty(),
            "Memory regions still exist after cleanup");
    }

    fn setup_memory_region(cage: &Cage, start: usize, size: usize, permissions: u32) -> Result<(), &'static str> {
        if size == 0 {
            return Err("Memory region size must be positive");
        }
        if permissions & (PROT_READ | PROT_WRITE | PROT_EXEC) != permissions {
            return Err("Invalid permissions specified");
        }
        
        let mut vmmap = cage.vmmap.write();
        vmmap.map_memory(start, size, permissions)
            .map_err(|_| "Failed to map memory region")
    }

    fn verify_memory_access(cage: &Cage, start: usize, size: usize, expected_perms: u32) {
        assert!(size > 0, "Memory region size must be positive");
        
        let vmmap = cage.vmmap.read();
        let region = vmmap.get_region(start)
            .expect("Memory region not found");
        
        assert_eq!(region.permissions, expected_perms, 
            "Incorrect permissions for memory region");
        assert!(region.contains_range(start, size), 
            "Memory region does not contain expected range");
        
        // Verify boundaries
        assert!(region.start <= start, "Region start boundary incorrect");
        assert!(region.start + region.size >= start + size, "Region end boundary incorrect");
    }
}