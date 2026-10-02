package app.ratatoskr.android

import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.util.TypedValue
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.android.material.button.MaterialButton

abstract class MobileActivity : AppCompatActivity() {
    fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
    val dark get() = resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK == android.content.res.Configuration.UI_MODE_NIGHT_YES
    val ink get() = Color.parseColor(if (dark) "#EEE5D7" else "#302D28")
    val paper get() = Color.parseColor(if (dark) "#131820" else "#EAE5DC")
    val surface get() = Color.parseColor(if (dark) "#202731" else "#F4F0E8")
    val accent get() = Color.parseColor(when (MobilePreferences(this).brand) {
        "midnight-arcane" -> if (dark) "#C3A0EB" else "#673E9D"
        "forest-rune" -> if (dark) "#BAD47D" else "#526B1C"
        "frost-byte" -> if (dark) "#90D5F0" else "#14628D"
        else -> if (dark) "#EBBA75" else "#955311"
    })
    override fun onCreate(savedInstanceState: Bundle?) {
        val prefs = MobilePreferences(this)
        val mode = when (prefs.mode) { "dark" -> AppCompatDelegate.MODE_NIGHT_YES; "light" -> AppCompatDelegate.MODE_NIGHT_NO; else -> AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM }
        // Scope configuration to this activity; changing a global default during
        // another activity's recreation can request overlapping recreations.
        delegate.localNightMode = mode
        if (AppCompatDelegate.getApplicationLocales().toLanguageTags() != prefs.language)
            AppCompatDelegate.setApplicationLocales(LocaleListCompat.forLanguageTags(prefs.language))
        appliedAppearance = listOf(prefs.mode, prefs.brand, prefs.language)
        super.onCreate(savedInstanceState)
    }
    private var appliedAppearance = emptyList<String>()
    override fun onResume() {
        super.onResume()
        val prefs = MobilePreferences(this)
        if (appliedAppearance != listOf(prefs.mode, prefs.brand, prefs.language)) {
            window.decorView.post { if (!isFinishing && !isDestroyed) recreate() }
        }
    }
    val muted get() = (ink and 0x00FFFFFF) or 0xC0000000.toInt()
    val success get() = Color.parseColor(if (dark) "#6CCB8F" else "#1F7A45")
    val danger get() = Color.parseColor(if (dark) "#F08A7A" else "#B3392A")
    /** A rounded rectangle, optionally with a thin outline. */
    fun rounded(fill: Int, radius: Int = 16, stroke: Int? = null) = GradientDrawable().apply {
        setColor(fill); cornerRadius = dp(radius).toFloat(); if (stroke != null) setStroke(dp(1), stroke)
    }
    /** A small coloured label such as a status. */
    fun chip(value: String, color: Int) = TextView(this).apply {
        text = value; textSize = 12f; setTextColor(color); setPadding(dp(10), dp(3), dp(10), dp(3))
        background = rounded((color and 0x00FFFFFF) or 0x2A000000, 20)
    }
    /** A round icon button with a ripple, tinted with the brand colour. */
    fun icon(res: Int, description: String, tint: Int = accent, size: Int = 48, action: () -> Unit) = ImageButton(this).apply {
        setImageResource(res); contentDescription = description; setColorFilter(tint)
        val ripple = TypedValue(); theme.resolveAttribute(android.R.attr.selectableItemBackgroundBorderless, ripple, true)
        setBackgroundResource(ripple.resourceId)
        layoutParams = LinearLayout.LayoutParams(dp(size), dp(size)); setOnClickListener { action() }
    }
    fun column() = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
    fun label(value: String, size: Float = 16f) = TextView(this).apply { text = value; textSize = size; setTextColor(ink); setPadding(0, dp(8), 0, dp(8)) }
    fun button(value: String, action: () -> Unit) = MaterialButton(this).apply {
        text = value; minHeight = dp(48); cornerRadius = dp(12)
        backgroundTintList = ColorStateList.valueOf(surface); setTextColor(accent)
        setOnClickListener { action() }
    }
    fun insets(view: android.view.View) {
        ViewCompat.setOnApplyWindowInsetsListener(view) { target, inset ->
            val bars = inset.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime())
            target.setPadding(dp(20) + bars.left, dp(12) + bars.top, dp(20) + bars.right, dp(16) + bars.bottom)
            inset
        }
    }
}
