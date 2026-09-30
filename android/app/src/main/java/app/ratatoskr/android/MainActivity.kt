package app.ratatoskr.android

import android.content.Intent
import android.os.Bundle
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The home screen: how to use the app, a paste button, and the engine update. */
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val box = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(48, 96, 48, 48)
        }
        box.addView(TextView(this).apply { text = getString(R.string.main_title); textSize = 24f })
        box.addView(TextView(this).apply { text = getString(R.string.main_hint); setPadding(0, 24, 0, 32) })
        box.addView(Button(this).apply {
            text = getString(R.string.paste)
            setOnClickListener {
                startActivity(
                    Intent(this@MainActivity, ShareActivity::class.java)
                        .putExtra(ShareActivity.EXTRA_FROM_CLIPBOARD, true),
                )
            }
        })
        val update = Button(this).apply { text = getString(R.string.update_engine) }
        update.setOnClickListener {
            update.isEnabled = false
            update.text = getString(R.string.updating)
            lifecycleScope.launch {
                val ok = try { withContext(Dispatchers.IO) { Engine.update(this@MainActivity) } } catch (e: Exception) { false }
                update.isEnabled = true
                update.text = getString(R.string.update_engine)
                Toast.makeText(
                    this@MainActivity,
                    if (ok) R.string.updated else R.string.update_failed,
                    Toast.LENGTH_SHORT,
                ).show()
            }
        }
        box.addView(update)
        setContentView(box)
    }
}
