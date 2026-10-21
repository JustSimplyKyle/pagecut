package dev.dioxus.main

import android.app.NativeActivity
import android.content.Intent

// iced runs through winit's NativeActivity backend rather than dx's WryActivity.
class MainActivity : NativeActivity() {
    companion object {
        init {
            // Registers the JNI entry points below; NativeActivity reuses this load.
            System.loadLibrary("main")
        }
    }

    // loki-file-access starts the document pickers with these request codes.
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == 1001 || requestCode == 1002) {
            onPickResult(if (resultCode == RESULT_OK) data?.data?.toString() else null)
        }
    }

    private external fun onPickResult(uri: String?)
}
