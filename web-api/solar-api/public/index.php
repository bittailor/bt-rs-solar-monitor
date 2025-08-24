<?php

require_once __DIR__.'/../vendor/autoload.php';

Flight::path(__DIR__.'/../');


// Then define a route and assign a function to handle the request.
Flight::route('/', function () {
  echo '<ul><li><a href="/api/v1/solar">solar</a></li><li><a href="/api/v1/lte">lte</a></li></ul>';
});

Flight::group('/api/v1', function () {
    Flight::route('/solar', function () {
        Flight::response()->setHeader('Content-Type', 'text/plain');
        Flight::response()->write("solar data");
    });
    Flight::route('/lte', function () {
        Flight::response()->setHeader('Content-Type', 'text/plain');
        Flight::response()->write("lte data");
    });
});

// Finally, start the framework.
Flight::start();