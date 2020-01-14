#include <winrt/Windows.System.h>
#include <DispatcherQueue.h>

using namespace winrt;
using namespace Windows::System;

namespace abi = ABI::Windows::System;

/// Perform a one-time initialization for this module. Must be called on a main
/// thread.
extern "C" HRESULT tcw_comp_init() {
	// Create a dispatcher queue for the current thread
	DispatcherQueueOptions options {
		sizeof(DispatcherQueueOptions),
		DQTYPE_THREAD_CURRENT,
		DQTAT_COM_ASTA,
	};

	static DispatcherQueueController ctrler{nullptr};
	return CreateDispatcherQueueController(
		options, 
		reinterpret_cast<abi::IDispatcherQueueController**>(put_abi(ctrler))
	);
}
