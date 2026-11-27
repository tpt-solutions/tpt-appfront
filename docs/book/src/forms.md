# Forms

`FormState` holds the values, validators and errors of a form. Each field is a signal, so inputs and
error messages update as the user types.

```rust,ignore
use tpt_appfront_core::FormState;

let form = FormState::new();
let email = form.field("email", "");
form.validate("email", |v| {
    if v.contains('@') { None } else { Some("Enter an email address".into()) }
});

// Bind the field to an input. On every backend the input dispatches its
// on_input message, and the app writes the new value back with form.set.
let view = UITree::container(|c| {
    c.input(email.get())
        .attr("aria-label", "Email")
        .on_input(|v| Msg::EmailChanged(v));
});

// Later, in the handler for EmailChanged:
form.set("email", value);

if form.is_valid() {
    submit(form.values());
}
```

- `field(name, default)` returns the field's `Signal<String>`. The first call creates the field.
- `validate(name, f)` runs `f` on each change. `None` means valid, and `Some(message)` is the error.
- `error(name)` returns the current message, if there is one.
- `checkbox(name, default)` and `set_checkbox` handle boolean fields.
- `is_valid()` checks every field, and `values()` returns all values by name.

Keep the rule and its message together with the field, so the error next to an input is always the
one its validator produced.
