package app.ratatoskr.android
import org.junit.Assert.*
import org.junit.Test
class FileNameSimilarityTest {
    @Test fun tagsAndNumberedCopiesAreHints() {
        assertTrue(FileNameSimilarity.similar("The.Mentalist.S04E01.720p.WEB-DL.x265.mkv", "The.Mentalist.S04E01.720p.WEB-DL.x265.MovieCottage.mkv"))
        assertTrue(FileNameSimilarity.similar("Show.S04E01.mkv", "Show.S04E01 (2).mkv"))
    }
    @Test fun otherEpisodesAndTypesAreNotMatches() {
        assertFalse(FileNameSimilarity.similar("Show.S04E01.mkv", "Show.S04E02.mkv"))
        assertFalse(FileNameSimilarity.similar("Show.S04E01.zip", "Show.S04E01.mkv"))
        assertFalse(FileNameSimilarity.similar("notes.txt", "other.txt"))
    }
}
