<?php

use Bt\Solar\Upload;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;
use Illuminate\Http\Middleware\HandleCors;
use App\Http\Middleware\StripToMinimalHeaders;
use App\Http\Controllers\SolarReadingController;
use App\Http\Middleware\ApiToken;

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

Route::middleware([ApiToken::class])->group(function () {
    Route::post('/v2/solar/reading', [SolarReadingController::class, 'reading'])->middleware([StripToMinimalHeaders::class]);
    Route::post('/v2/solar/event', [SolarReadingController::class, 'event'])->middleware([StripToMinimalHeaders::class]);
});



