package app.ratatoskr.android

import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.icu.util.Calendar
import android.icu.util.TimeZone
import android.icu.util.ULocale
import android.widget.LinearLayout
import android.widget.NumberPicker
import android.widget.Toast
import androidx.appcompat.app.AlertDialog

/** The calendar changes presentation, never the instant stored by the scheduler. */
object MobileDates {
    fun calendar(persian: Boolean, instant: Long): Calendar = Calendar.getInstance(TimeZone.getDefault(),
        ULocale(if (persian) "fa_IR@calendar=persian" else "en_US@calendar=gregorian")).apply { timeInMillis = instant }
    fun instant(persian: Boolean, year: Int, month: Int, day: Int, hour: Int = 0, minute: Int = 0): Long =
        calendar(persian, 0).apply { clear(); isLenient = false; set(year, month, day, hour, minute, 0) }.timeInMillis
    fun dayStart(instant: Long) = calendar(false, instant).apply {
        set(Calendar.HOUR_OF_DAY, 0); set(Calendar.MINUTE, 0); set(Calendar.SECOND, 0); set(Calendar.MILLISECOND, 0)
    }.timeInMillis
    fun nextDay(instant: Long) = calendar(false, dayStart(instant)).apply { add(Calendar.DATE, 1) }.timeInMillis
    fun format(context: android.content.Context, instant: Long): String {
        val persian = MobilePreferences(context).calendarType == "persian"
        val cal = calendar(persian, instant)
        return cal.getDateTimeFormat(android.icu.text.DateFormat.MEDIUM, android.icu.text.DateFormat.SHORT,
            ULocale(if (persian) "fa_IR@calendar=persian" else "en_US")).format(java.util.Date(instant))
    }
    fun choose(activity: MobileActivity, initial: Long = System.currentTimeMillis() + 3_600_000,
        dateOnly: Boolean = false, onChosen: (Long) -> Unit) {
        run {
            val persian = MobilePreferences(activity).calendarType == "persian"
            val cal = calendar(persian, initial)
            fun complete(year: Int, month: Int, day: Int) {
                fun save(hour: Int, minute: Int) {
                    val result = runCatching { instant(persian, year, month, day, hour, minute) }.getOrNull()
                    if (result == null || (!dateOnly && result <= System.currentTimeMillis())) {
                        Toast.makeText(activity, R.string.schedule_invalid, Toast.LENGTH_LONG).show()
                    } else onChosen(result)
                }
                if (dateOnly) save(0, 0)
                else TimePickerDialog(activity, { _, hour, minute -> save(hour, minute) }, cal.get(Calendar.HOUR_OF_DAY),
                    cal.get(Calendar.MINUTE), android.text.format.DateFormat.is24HourFormat(activity)).show()
            }
            if (!persian) DatePickerDialog(activity, { _, year, month, day -> complete(year, month, day) },
                cal.get(Calendar.YEAR), cal.get(Calendar.MONTH), cal.get(Calendar.DAY_OF_MONTH)).show()
            else {
                val box = LinearLayout(activity).apply { orientation = LinearLayout.HORIZONTAL; setPadding(activity.dp(16), 0, activity.dp(16), 0) }
                fun picker(label: String, min: Int, max: Int, value: Int) = NumberPicker(activity).apply {
                    minValue = min; maxValue = max; this.value = value; wrapSelectorWheel = false; contentDescription = label
                    box.addView(this, LinearLayout.LayoutParams(0, activity.dp(180), 1f))
                }
                val year = picker(activity.getString(R.string.date_year), 1300, 1600, cal.get(Calendar.YEAR).coerceIn(1300, 1600))
                val month = picker(activity.getString(R.string.date_month), 1, 12, cal.get(Calendar.MONTH) + 1).apply {
                    displayedValues = activity.resources.getStringArray(R.array.persian_months)
                }
                val day = picker(activity.getString(R.string.date_day), 1, cal.getActualMaximum(Calendar.DAY_OF_MONTH), cal.get(Calendar.DAY_OF_MONTH))
                fun adjust() {
                    val first = calendar(true, instant(true, year.value, month.value - 1, 1))
                    day.maxValue = first.getActualMaximum(Calendar.DAY_OF_MONTH)
                }
                year.setOnValueChangedListener { _, _, _ -> adjust() }; month.setOnValueChangedListener { _, _, _ -> adjust() }
                AlertDialog.Builder(activity).setTitle(R.string.calendar_persian).setView(box).setNegativeButton(R.string.cancel, null)
                    .setPositiveButton(android.R.string.ok) { _, _ -> box.clearFocus(); complete(year.value, month.value - 1, day.value) }.show()
            }
        }
    }
}
