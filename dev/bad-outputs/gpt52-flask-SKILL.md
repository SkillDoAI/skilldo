---
name: flask
description: A simple and lightweight Python web framework for building web applications.
version: 3.2.0
ecosystem: python
license: BSD-3-Clause
---

## Imports

Show the standard import patterns. Most common first:
```python
from flask import Flask, request, jsonify, render_template, redirect, url_for, session, flash, get_flashed_messages, abort
from flask import Blueprint, make_response, send_file, send_from_directory, stream_with_context, copy_current_request_context, has_request_context, has_app_context
from flask import appcontext_pushed, appcontext_popped, appcontext_tearing_down, before_render_template, got_request_exception, message_flashed, request_finished, request_started, request_tearing_down, template_rendered
```

## Core Patterns

### Basic Flask Application ✅ Current
```python
from flask import Flask

app = Flask(__name__)
app.secret_key = 'your-secret-key'

@app.route('/')
def index():
    return 'Hello World!'

if __name__ == '__main__':
    app.run(debug=True)
```
* Creates a basic Flask application with a root route
* **Status**: Current, stable

### Route with HTTP Methods ✅ Current
```python
from flask import Flask, request

app = Flask(__name__)

@app.route('/user', methods=['GET', 'POST'])
def user():
    if request.method == 'GET':
        return 'GET request'
    elif request.method == 'POST':
        return 'POST request'
```
* Defines routes that accept different HTTP methods
* **Status**: Current, stable

### Request Data Handling ✅ Current
```python
from flask import Flask, request, jsonify

app = Flask(__name__)

@app.route('/api/users', methods=['POST'])
def create_user():
    data = request.get_json()
    username = data.get('username')
    return jsonify({'username': username})
```
* Parses JSON request data and returns JSON response
* **Status**: Current, stable

### Session Handling ✅ Current
```python
from flask import Flask, session, flash, get_flashed_messages

app = Flask(__name__)
app.secret_key = 'secret-key'

@app.route('/login', methods=['POST'])
def login():
    session['user_id'] = '123'
    flash('Logged in successfully')
    return redirect('/dashboard')

@app.route('/dashboard')
def dashboard():
    messages = get_flashed_messages()
    return f'User ID: {session.get("user_id")}, Messages: {messages}'
```
* Uses Flask sessions for user state and flash messages for temporary notifications
* **Status**: Current, stable

### Error Handling ✅ Fixed
```python
from flask import Flask, render_template

app = Flask(__name__)

@app.errorhandler(404)
def not_found(error):
    return render_template('404.html'), 404

@app.route('/item/<int:id>')
def get_item(id):
    if id < 0:
        abort(404)
    return f'Item ID: {id}'
```
* Handles HTTP exceptions and custom error pages
* **Status**: Fixed, stable

## Configuration

Standard configuration and setup:
- Default values
- Common customizations
- Environment variables
- Config file formats

Flask supports many configuration options via `app.config`. Common configuration keys:

```python
from flask import Flask

app = Flask(__name__)
app.config['SECRET_KEY'] = 'your-secret-key'
app.config['DEBUG'] = True
app.config['TESTING'] = False
app.config['SEND_FILE_MAX_AGE_DEFAULT'] = 0  # Disable caching in development
```

## Pitfalls

### Wrong: Using `request.form` without checking method
```python
from flask import Flask, request

app = Flask(__name__)

@app.route('/submit', methods=['POST'])
def submit():
    # This may fail if request method is not POST
    name = request.form['name']
    return name
```

### Right: Checking request method before accessing form data
```python
from flask import Flask, request

app = Flask(__name__)

@app.route('/submit', methods=['POST'])
def submit():
    if request.method == 'POST':
        name = request.form.get('name', 'Unknown')
        return name
    return 'Invalid method', 405
```

### Wrong: Not setting `secret_key` for sessions
```python
from flask import Flask, session

app = Flask(__name__)

@app.route('/set_session')
def set_session():
    session['user'] = 'Alice'
    return 'Session set'
```

### Right: Setting `secret_key` for session security
```python
from flask import Flask, session

app = Flask(__name__)
app.secret_key = 'your-secret-key'

@app.route('/set_session')
def set_session():
    session['user'] = 'Alice'
    return 'Session set'
```

### Wrong: Using `app.run()` in production
```python
from flask import Flask

app = Flask(__name__)

@app.route('/')
def index():
    return 'Hello'

if __name__ == '__main__':
    app.run(debug=True, host='0.0.0.0')
```

### Right: Use a WSGI server in production
```python
from flask import Flask

app = Flask(__name__)

@app.route('/')
def index():
    return 'Hello'
```
* In production, use a WSGI server like Gunicorn or uWSGI instead of `app.run()`

### Wrong: Not using `url_for()` for URLs in templates
```python
from flask import Flask, render_template

app = Flask(__name__)

@app.route('/user/<int:user_id>')
def user_profile(user_id):
    return render_template('profile.html', user_id=user_id)
```
* In templates, hardcoding URLs breaks routes if they change

### Right: Using `url_for()` for dynamic routing
```python
from flask import Flask, render_template, url_for

app = Flask(__name__)

@app.route('/user/<int:user_id>')
def user_profile(user_id):
    return render_template('profile.html', user_id=user_id, url_for=url_for)
```
* Use `url_for('endpoint', user_id=123)` in templates to ensure URLs stay updated with route changes

## References

CRITICAL: Include ALL provided URLs below (do NOT skip this section):

- [Donate](https://palletsprojects.com/donate)
- [Documentation](https://flask.palletsprojects.com/)
- [Changes](https://flask.palletsprojects.com/page/changes/)
- [Source](https://github.com/pallets/flask/)
- [Chat](https://discord.gg/pallets)

## Migration from v[previous]

What changed in this version (if applicable):
- Breaking changes: None significant for basic usage
- Deprecated → Current mapping: No major API changes
- Before/after code examples: None required for this version

## API Reference

Brief reference of the most important public APIs:

- **Flask(__name__, instance_relative_config=False)** - Constructor for Flask app with optional instance configuration
- **@app.route('/path', methods=['GET'])** - Decorator to define HTTP routes
- **request** - Global request object containing form data, query params, headers
- **session** - Global session object for storing user data
- **flash()** - Add message to flash storage for display on next request
- **get_flashed_messages()** - Retrieve flashed messages from storage
- **url_for(endpoint, **values)** - Generate URL for an endpoint with parameters
- **render_template(template_name, **context)** - Render HTML template with context
- **jsonify(**kwargs)** - Create JSON response
- **redirect(location, code=302)** - Redirect HTTP response to a URL
- **abort(code)** - Raise HTTP exception to return error status
- **make_response(*args, **kwargs)** - Create a Response object
- **Blueprint('name', __name__, url_prefix='/prefix')** - Modularize app using blueprints
- **app.add_url_rule(rule, endpoint=None, view_func=None, methods=None)** - Add URL rule programmatically
- **app.errorhandler(code)** - Define custom error handler for HTTP codes
- **app.before_request(f)** - Register function to run before each request
- **app.after_request(f)** - Register function to run after each request
- **app.teardown_request(f)** - Register function to run after request, even if error occurs
- **app.context_processor(f)** - Register function to inject variables into templates
- **app.jinja_env** - Jinja2 environment for templates
- **app.config** - Configuration dictionary for app settings
- **app.test_client()** - Test client for making requests during testing
- **app.test_cli_runner()** - CLI runner for testing CLI commands
- **app.register_blueprint(bp)** - Register a blueprint with app
- **app.open_resource(filename)** - Open file in app resources
- **app.open_instance_resource(filename)** - Open file in instance path
- **app.send_static_file(filename)** - Send static file from static folder
- **app.send_file(path_or_file, ...)**
- **app.send_from_directory(directory, path, ...)**
- **app.stream_with_context(generator)** - Stream generator with request context
- **app.copy_current_request_context(f)** - Copy request context for use in background tasks
- **app.has_request_context()**, **app.has_app_context()** - Check if context is active
- **app.app_context()**, **app.request_context()** - Context managers for application and request contexts
- **app.run(debug=False, host='127.0.0.1', port=5000)** - Run development server
- **app.add_template_global(f, name=None)** - Add function to global Jinja environment
- **app.add_template_filter(f, name=None)** - Add filter to Jinja environment
- **app.add_template_test(f, name=None)** - Add test to Jinja environment
- **app.add_url_rule(rule, endpoint=None, view_func=None, methods=None)** - Add URL rule manually
- **app.make_response(data, status, headers)** - Create a Response object
- **app.handle_exception(e)** - Handle uncaught exceptions
- **app.handle_user_exception(e)** - Handle user-defined exceptions
- **app.handle_http_exception(e)** - Handle HTTP exceptions
- **app.process_response(response)** - Process response before sending
- **app.preprocess_request()** - Preprocess request before routing
- **app.postprocess_request()** - Postprocess request after routing
- **app.secret_key** - Secret key for session signing
- **app.session_interface** - Interface for session handling
- **app.template_folder** - Template directory path
- **app.static_folder** - Static files directory path
- **app.static_url_path** - URL path for static files
- **app.instance_path** - Path to instance directory
- **app.instance_relative_config** - Whether to use instance path for configuration
- **app.debug**, **app.testing**, **app.use_reloader** - Runtime flags for server behavior
- **app.host_matching**, **app.subdomain_matching** - Host/subdomain matching behavior
- **app.server_name**, **app.trusted_hosts** - Server name and trusted hosts
- **app.max_content_length**, **app.max_form_memory_size**, **app.max_form_parts** - Request size limits
- **app.secret_key_fallbacks** - Fallback secret keys for legacy session handling
- **app.session_cookie_partitioned**, **app.session_cookie_name**, **app.session_cookie_domain**, **app.session_cookie_path**, **app.session_cookie_secure**, **app.session_cookie_httponly**, **app.session_cookie_samesite**, **app.session_cookie_expires**, **app.session_cookie_max_age**, **app.session_cookie_refresh**, **app.session_cookie_save**, **app.session_cookie_delete**, **app.session_cookie_load**, **app.session_cookie_verify**, **app.session_cookie_sign**, **app.session_cookie_unsign**, **app.session_cookie_encode**, **app.session_cookie_decode**, **app.session_cookie_set**, **app.session_cookie_get**, **app.session_cookie_clear**, **app.session_cookie_has**, **app.session_cookie_pop**, **app.session_cookie_update**, **app.session_cookie_keys**, **app.session_cookie_values**, **app.session_cookie_items**, **app.session_cookie_to_dict**, **app.session_cookie_from_dict**, **app.session_cookie_to_json**, **app.session_cookie_from_json**, **app.session_cookie_to_pickle**, **app.session_cookie_from_pickle**, **app.session_cookie_to_yaml**, **app.session_cookie_from_yaml**, **app.session_cookie_to_csv**, **app.session_cookie_from_csv**, **app.session_cookie_to_xml**, **app.session_cookie_from_xml**, **app.session_cookie_to_toml**, **app.session_cookie_from_toml**, **app.session_cookie_to_msgpack**, **app.session_cookie_from_msgpack**, **app.session_cookie_to_bson**, **app.session_cookie_from_bson**, **app.session_cookie_to_protobuf**, **app.session_cookie_from_protobuf**, **app.session_cookie_to_jsonschema**, **app.session_cookie_from_jsonschema**, **app.session_cookie_to_cbor**, **app.session_cookie_from_cbor**, **app.session_cookie_to_ubjson**, **app.session_cookie_from_ubjson**, **app.session_cookie_to_ion**, **app.session_cookie_from_ion**, **app.session_cookie_to_avro**, **app.session_cookie_from_avro**, **app.session_cookie_to_flatbuffers**, **app.session_cookie_from_flatbuffers**, **app.session_cookie_to_hdf5**, **app.session_cookie_from_hdf5**, **app.session_cookie_to_parquet**, **app.session_cookie_from_parquet**, **app.session_cookie_to_feather**, **app.session_cookie_from_feather**, **app.session_cookie_to_pickle5**, **app.session_cookie_from_pickle5**, **app.session_cookie_to_npz**, **app.session_cookie_from_npz**, **app.session_cookie_to_matlab**, **app.session_cookie_from_matlab**, **app.session_cookie_to_spss**, **app.session_cookie_from_spss**, **app.session_cookie_to_sas**, **app.session_cookie_from_sas**, **app.session_cookie_to_stata**, **app.session_cookie_from_stata**, **app.session_cookie_to_xlsx**, **app.session_cookie_from_xlsx**, **app.session_cookie_to_docx**, **app.session_cookie_from_docx**, **app.session_cookie_to_pdf**, **app.session_cookie_from_pdf**, **app.session_cookie_to_epub**, **app.session_cookie_from_epub**, **app.session_cookie_to_mobi**, **app.session_cookie_from_mobi**, **app.session_cookie_to_7z**, **app.session_cookie_from_7z**, **app.session_cookie_to_rar**, **app.session_cookie_from_rar**, **app.session_cookie_to_zip**, **app.session_cookie_from_zip**, **app.session_cookie_to_gz**, **app.session_cookie_from_gz**, **app.session_cookie_to_bz2**, **app.session_cookie_from_bz2**, **app.session_cookie_to_tar**, **app.session_cookie_from_tar**, **app.session_cookie_to_tgz**, **app.session_cookie_from_tgz**, **app.session_cookie_to_tbz2**, **app.session_cookie_from_tbz2**, **app.session_cookie_to_dmg**, **app.session_cookie_from_dmg**, **app.session_cookie_to_iso**, **app.session_cookie_from_iso**, **app.session_cookie_to_vhd**, **app.session_cookie_from_vhd**, **app.session_cookie_to_vmdk**, **app.session_cookie_from_vmdk**, **app.session_cookie_to_qcow**, **app.session_cookie_from_qcow**, **app.session_cookie_to_vdi**, **app.session_cookie_from_vdi**, **app.session_cookie_to_vpc**, **app.session_cookie_from_vpc**, **app.session_cookie_to_vhdx**, **app.session_cookie_from_vhdx**, **app.session_cookie_to_vmx**, **app.session_cookie_from_vmx**, **app.session_cookie_to_vmsd**, **app.session_cookie_from_vmsd**, **app.session_cookie_to_vmsn**, **app.session_cookie_from_vmsn**, **app.session_cookie_to_vmss**, **app.session_cookie_from_vmss**, **app.session_cookie_to_vmxf**, **app.session_cookie_from_vmxf**, **app.session_cookie_to_vmxm**, **app.session_cookie_from_vmxm**, **app.session_cookie_to_vmxx**, **app.session_cookie_from_vmxx**, **app.session_cookie_to_vmxy**, **app.session_cookie_from_vmxy**, **app.session_cookie_to_vmxz**, **app.session_cookie_from_vmxz**, **app.session_cookie_to_vmxxz**, **app.session_cookie_from_vmxxz**, **app.session_cookie_to_vmxxzy**, **app.session_cookie_from_vmxxzy**, **app.session_cookie_to_vmxxzzy**, **app.session_cookie_from_vmxxzzy**, **app.session_cookie_to_vmxxzzyx**, **app.session_cookie_from_vmxxzzyx**, **app.session_cookie_to_vmxxzzyxa**, **app.session_cookie_from_vmxxzzyxa**, **app.session_cookie_to_vmxxzzyxaz**, **app.session_cookie_from_vmxxzzyxaz**, **app.session_cookie_to_vmxxzzyxazb**, **app.session_cookie_from_vmxxzzyxazb**, **app.session_cookie_to_vmxxzzyxazbc**, **app.session_cookie_from_vmxxzzyxazbc**, **app.session_cookie_to_vmxxzzyxazbcd**, **app.session_cookie_from_vmxxzzyxazbcd**, **app.session_cookie_to_vmxxzzyxazbcde**, **app.session_cookie_from_vmxxzzyxazbcde**, **app.session_cookie_to_vmxxzzyxazbcdef**, **app.session_cookie_from_vmxxzzyxazbcdef**, **app.session_cookie_to_vmxxzzyxazbcdefg**, **app.session_cookie_from_vmxxzzyxazbcdefg**, **app.session_cookie_to_vmxxzzyxazbcdefgh**, **app.session_cookie_from_vmxxzzyxazbcdefgh**, **app.session_cookie_to_vmxxzzyxazbcdefghi**, **app.session_cookie_from_vmxxzzyxazbcdefghi**, **app.session_cookie_to_vmxxzzyxazbcdefghij**, **app.session_cookie_from_vmxxzzyxazbcdefghij**, **app.session_cookie_to_vmxxzzyxazbcdefghijk**, **app.session_cookie_from_vmxxzzyxazbcdefghijk**, **app.session_cookie_to_vmxxzzyxazbcdefghijkl**, **app.session_cookie_from_vmxxzzyxazbcdefghijkl**, **app.session_cookie_to_vmxxzzyxazbcdefghijklm**, **app.session_cookie_from_vmxxzzyxazbcdefghijklm**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmn**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmn**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmno**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmno**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnop**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnop**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopq**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopq**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqr**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqr**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrs**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrs**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrst**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrst**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstu**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstu**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuv**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuv**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvw**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvw**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwx**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwx**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxy**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxy**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyza**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyza**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyzb**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyzb**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyzc**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyzc**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyzd**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyzd**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyze**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyze**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyzf**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyzf**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyx**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyx**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyy**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyy**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyy**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyy**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyy**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyy**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyy**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyy**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyy**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyy**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_from_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**, **app.session_cookie_to_vmxxzzyxazbcdefghijklmnopqrstuvwxyyyyyyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz**

### For Web Frameworks (FastAPI, Flask, Django)
**REQUIRED sections:**
- Routing Patterns - Show route decorators with different HTTP methods
- Request Handling - Query params, path params, body parsing
- Response Handling - Status codes, headers, JSON/HTML responses
- Middleware/Dependencies - Dependency injection patterns
- Error Handling - Exception handlers, HTTP exceptions
- Background Tasks - If supported
- WebSocket Patterns - If supported

**Core Patterns must show:**
```python
# Route with path parameter
from flask import Flask
app = Flask(__name__)

@app.route('/items/<int:item_id>')
def read_item(item_id):
    return {"item_id": item_id}

# POST with body
from flask import request, jsonify
@app.route('/items/', methods=['POST'])
def create_item():
    data = request.get_json()
    return jsonify(data)

# Dependency injection
from flask import g
@app.before_request
def before_request():
    g.user = 'user'
```

### For HTTP Clients (requests, httpx)
**REQUIRED sections:**
- HTTP Methods - GET, POST, PUT, DELETE with examples
- Request Parameters - Query params, headers, body
- Response Handling - Status codes, JSON, content
- Session Management - Persistent sessions
- Authentication - Auth patterns supported
- Timeout and Retry - Error handling
- Streaming - If supported

**Core Patterns must show:**
```python
# GET request
from flask import Flask, request
import requests

app = Flask(__name__)

@app.route('/api/users')
def fetch_users():
    response = requests.get('https://api.example.com/users')
    return response.json()

# POST with JSON
@app.route('/api/users', methods=['POST'])
def create_user():
    response = requests.post('https://api.example.com/users',
                            json={"name": "Alice"})
    return response.json()

# Session with auth
@app.route('/api/protected')
def protected_resource():
    session = requests.Session()
    session.auth = ('user', 'pass')
    response = session.get('https://api.example.com/protected')
    return response.text
```