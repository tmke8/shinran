use unicode_segmentation::UnicodeSegmentation;

pub fn process_cursor_hint(body: String) -> (String, Option<usize>) {
    if let Some(index) = body.find("$|$") {
        // Convert the byte index to a char index
        let char_str = &body[0..index];
        let char_index = char_str.graphemes(true).count();
        let total_size = body.graphemes(true).count();

        // Remove the $|$ placeholder
        let body = body.replace("$|$", "");

        // Calculate the amount of rewind moves needed (LEFT ARROW).
        // Subtract also 3, equal to the number of chars of the placeholder "$|$"
        let moves = total_size - char_index - 3;
        (body, Some(moves))
    } else {
        (body, None)
    }
}
