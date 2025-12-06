<?php

use Bt\Solar\Upload;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;
use Illuminate\Http\Middleware\HandleCors;
use App\Http\Middleware\StripToMinimalHeaders;
use App\Http\Controllers\SolarReadingController;

/*
|--------------------------------------------------------------------------
| API Routes
|--------------------------------------------------------------------------
|
| Here is where you can register API routes for your application. These
| routes are loaded by the RouteServiceProvider and all of them will
| be assigned to the "api" middleware group. Make something great!
|
*/

Route::middleware('auth:sanctum')->get('/user', function (Request $request) {
    return $request->user();
});

Route::get('/v2/info', function (Request $request) {
    return "solar api v2";
});

Route::post('/v2/solar/reading', [SolarReadingController::class, 'upload'])->middleware([StripToMinimalHeaders::class]);

Route::post('/v2/solar', function (Request $request) {
    $content = $request->getContent();
    $upload = new Upload();
    $upload->mergeFromString($content);
    $n = $upload->getEntries()->count();
    return "$n entries received";
});

/*
Flight::group('/api/v2', function () {
    Flight::route('POST /solar', function () {
        $msg = Flight::request()->getBody();
        $upload = new Upload();
        $upload->mergeFromString($msg);
        $n = $upload->getEntries()->count();
        Flight::response()->setHeader('Content-Type', 'x');
        Flight::response()->write("$n entries received");
    });
});
*/