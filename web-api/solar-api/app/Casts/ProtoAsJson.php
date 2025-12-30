<?php

namespace App\Casts;

use Illuminate\Contracts\Database\Eloquent\CastsAttributes;
use Illuminate\Database\Eloquent\Model;

class ProtoAsJson implements CastsAttributes
{
    public function __construct(protected string|null $class = null) 
    {
    }


    /**
     * Cast the given value.
     *
     * @param  array<string, mixed>  $attributes
     */
    public function get(Model $model, string $key, mixed $value, array $attributes): mixed
    {
        $message = new $this->class();
        $message->mergeFromJsonString($value);
        return $message;
    }

    /**
     * Prepare the given value for storage.
     *
     * @param  array<string, mixed>  $attributes
     */
    public function set(Model $model, string $key, mixed $value, array $attributes): mixed
    {
        $json = $value->serializeToJsonString();
        return [$key => $json];
    }
}
