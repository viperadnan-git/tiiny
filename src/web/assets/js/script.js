const typewriter_element = document.getElementById('typewriter');
const url_input_element = $('#form input[type=url]');
const key_input_element = $('#form input[type=text]');
const submit_btn_element = $('#form button[type=submit]');
const url_show_element = $('#showcase input[type=url]');
const copy_btn_element = $("#showcase button[type=button]");

const Toast = Swal.mixin({
    toast: true,
    position: 'top',
    showConfirmButton: false,
    timer: 3000,
    // timerProgressBar: true,
    showCloseButton: true,
    didOpen: (toast) => {
        toast.addEventListener('mouseenter', Swal.stopTimer)
        toast.addEventListener('mouseleave', Swal.resumeTimer)
    }
})

Array.prototype.random = function () {
    return this[Math.floor(Math.random() * this.length)];
}


const keywords = ["affiliators", "Memers", "Astronauts", "Elon Musk", "influencers", "YouTubers", "programmers", "developers"];
const typewriter = new Typewriter(typewriter_element);
typewriter.start()
setInterval(() => {
    typewriter
        .deleteAll()
        .typeString(keywords.random() + '.')
        .pauseFor(1500);
})


$('#form').submit(function (e) {
    e.preventDefault();
    submit_btn_element.text('Shortening...');
    let data = {
        url: url_input_element.val()
    }
    if (key_input_element.val()) {
        data.key = key_input_element.val()
    }
    let options = {
        type: 'POST',
        url: '/api',
        data: JSON.stringify(data),
        headers: {
            "Content-Type": "application/json"
        }
    }
    $.ajax(options)
        .done((data) => {
            $('#form').hide();
            url_show_element.val(data.url)
            $('#showcase').removeClass('d-none');
        })
        .fail((request, status, error) => {
            let error_message = request.responseText;
            try {
                error_message = JSON.parse(error_message).error
            } catch (e) { console.log(e); }
            Toast.fire({
                title: error,
                text: error_message,
                icon: 'error'
            })
        }).always(() => {
            submit_btn_element.text('Make it Tiiny');
        })
});

$('#mode').click((event) => {
    var curr_mode = Cookies.get('__TIINY_MODE');
    if (curr_mode == 'dark') {
        Cookies.set('__TIINY_MODE', 'light', { expires: 365 })
    } else {
        Cookies.set('__TIINY_MODE', 'dark', { expires: 365 })
    }
    window.location.reload()
})

function copy_url() {
    var copyText = url_show_element[0];
    copyText.select();
    copyText.setSelectionRange(0,
        99999);
    document.execCommand("copy");
    copy_btn_element.text("Copied")
}

function reset() {
    $('#showcase').addClass('d-none');
    $('#form').show();
    url_input_element.val('')
    copy_btn_element.text("Copy")
}