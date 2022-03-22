use crate::color;

pub struct TemplateStrings {
    pub start_message_template: String,
    pub done_message_template: String,
    pub payload_message_template: String,
    pub error_message_template: String,
}

pub struct Template {
    pub name: String,
    pub begin_color: String,
    pub reset_color: String,
    pub error_message: String,
    pub status_code: Option<i32>,
    pub handle_flag: String,
}

impl Template {
    pub fn new(msg_color: Option<&color::Color>) -> Template {
        Template {
            name: String::new(),
            begin_color: if let Some(col) = msg_color {
                color::open_sequence(col)
            } else {
                String::new()
            },
            reset_color: if msg_color.is_some() {
                color::close_sequence()
            } else {
                String::new()
            },
            error_message: String::new(),
            status_code: None,
            handle_flag: String::new(),
        }
    }

    pub fn execute(&self, template_string: &str) -> String {
        let status_code_message = if self.status_code.is_some() {
            format!("{}", self.status_code.unwrap())
        } else {
            "(none)".to_string()
        };
        template_string
            .replace("{{name}}", &self.name)
            .replace("{{begin_color}}", &self.begin_color)
            .replace("{{reset_color}}", &self.reset_color)
            .replace("{{error_message}}", &self.error_message)
            .replace("{{status_code}}", &status_code_message)
            .replace("{{handle_flag}}", &self.handle_flag)
    }
}
