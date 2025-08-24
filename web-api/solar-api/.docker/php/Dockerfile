FROM php:8.1-apache-bookworm

RUN mv "$PHP_INI_DIR/php.ini-development" "$PHP_INI_DIR/php.ini"

RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends --no-install-suggests \ 
        git ruby openssh-client rsync default-mysql-client mariadb-client zip unzip libpng-dev \
    && apt-get clean -y && rm -rf /var/lib/apt/lists/*

RUN gem install net-ssh

RUN pecl install xdebug \
    && docker-php-ext-enable xdebug \
    && docker-php-ext-install mysqli \
    && docker-php-ext-install pdo_mysql \
    && docker-php-ext-install gd \
    && rm -rf /tmp/pear

RUN curl -sS https://getcomposer.org/installer | php -- --install-dir=/usr/local/bin --filename=composer

RUN a2enmod rewrite && a2enmod negotiation

ENV APACHE_DOCUMENT_ROOT="/var/workspace/bt-rs-solar-monitor/web-api/solar-api/public"

RUN sed -ri -e 's!/var/www/html!${APACHE_DOCUMENT_ROOT}!g' /etc/apache2/sites-available/*.conf
RUN sed -ri -e 's!/var/www/!${APACHE_DOCUMENT_ROOT}!g' /etc/apache2/apache2.conf /etc/apache2/conf-available/*.conf

RUN sh -c "$(curl -L https://github.com/deluan/zsh-in-docker/releases/download/v1.2.1/zsh-in-docker.sh)" -- \
    -t robbyrussell \
    -p git \
    -p rake-fast 


WORKDIR /var/workspace/bt-rs-solar-monitor
