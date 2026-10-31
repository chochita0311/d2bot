//! 개발 계측은 자체 프로세스의 수치만 읽는다. 힙/작업집합을 강제 정리하지 않는다.
use serde::Serialize;
use std::{io, mem::size_of};
use windows_sys::Win32::System::{
    Memory::{GetProcessHeap, HEAP_SUMMARY, HeapSummary},
    ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
    Threading::GetCurrentProcess,
};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ResourceUsage {
    pub working_set_bytes: usize,
    pub private_bytes: usize,
    pub default_heap_allocated_bytes: usize,
    pub default_heap_committed_bytes: usize,
    pub default_heap_reserved_bytes: usize,
}

/// Windows 10 build20348 이상에서 자체 기본 힙과 프로세스 자원을 읽는다.
/// 기본 힙 수치는 드라이버/별도 힙을 포함한 전체 프로세스 수치와 구분한다.
pub fn own_resources() -> io::Result<ResourceUsage> {
    let mut memory = PROCESS_MEMORY_COUNTERS_EX {
        cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    let mut heap = HEAP_SUMMARY {
        cb: size_of::<HEAP_SUMMARY>() as u32,
        ..Default::default()
    };
    // SAFETY: 현재 프로세스 pseudo-handle만 사용하고 크기가 명시된 C 레이아웃
    // 지역 출력 버퍼를 동기 호출 동안 유지한다. EX는 기본 counters를 앞에 포함한다.
    let observed = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            memory.cb,
        )
    };
    if observed == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: 소유권을 이전하지 않는 자체 기본 힙 handle이고 버퍼 크기/flags가 유효하다.
    if unsafe { HeapSummary(GetProcessHeap(), 0, &mut heap) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(ResourceUsage {
        working_set_bytes: memory.WorkingSetSize,
        private_bytes: memory.PrivateUsage,
        default_heap_allocated_bytes: heap.cbAllocated,
        default_heap_committed_bytes: heap.cbCommitted,
        default_heap_reserved_bytes: heap.cbReserved,
    })
}
