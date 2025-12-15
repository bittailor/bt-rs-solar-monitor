<?php

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

class ApiToken
{
    /**
     * Handle an incoming request.
     *
     * @param  \Closure(\Illuminate\Http\Request): (\Symfony\Component\HttpFoundation\Response)  $next
     */
    public function handle(Request $request, Closure $next): Response
    {
        $token = env('SOLAR_BACKEND_TOKEN'); 
        if ( $token === null) {
            return response('server misconfiguration', 500);
        }

        $requestToken = $request->header('X-Token');
        if ( $requestToken === null) {
            return response('missing token', 401);
        }

        if($requestToken !== env('SOLAR_BACKEND_TOKEN', '')) {
            return response('invalid token', 401);
        }

        return $next($request);
    }
}
