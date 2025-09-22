<?php

require_once __DIR__.'/../vendor/autoload.php';

Flight::path(__DIR__.'/../');


// Then define a route and assign a function to handle the request.
Flight::route('/', function () {
  echo '<ul><li><a href="/api/v1/solar">solar</a></li><li><a href="/api/v1/lte">lte</a></li></ul>';
});

Flight::route('/info', function () {
  phpinfo();
});

Flight::group('/api/v1', function () {
    Flight::route('GET /solar', function () {
        Flight::response()->setHeader('Content-Type', 'x');
        Flight::response()->write("solar data\r\n<one>\r\n<two>\r\n<tree>");
    });
    Flight::route('GET /lte', function () {
        Flight::response()->setHeader('Content-Type', 'x');
        Flight::response()->write("lte data");
    });
    Flight::route('POST /solar', function () {
        $msg = Flight::request()->getBody();
        $hex = bin2hex($msg);
        Flight::response()->setHeader('Content-Type', 'x');
        Flight::response()->write("solar data <- $msg [$hex]");
    });
    Flight::route('POST /lte', function () {
        $msg = Flight::request()->getBody();
        $hex = bin2hex($msg);
        Flight::response()->setHeader('Content-Type', 'x');
        Flight::response()->write("lte data <- $msg [$hex]");
    });
});

// Finally, start the framework.
Flight::start();